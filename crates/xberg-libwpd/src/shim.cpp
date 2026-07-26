/* Flat C shim over libwpd + librevenge for Xberg.
 *
 * libwpd exposes no `extract()` call. It drives librevenge's SAX-like
 * RVNGTextInterface: the caller passes a concrete implementation into
 * WPDocument::parse and libwpd invokes its callbacks. This file provides such
 * an implementation (DocumentBuilder) that records a flat, format-agnostic
 * internal document (a `std::vector<Node>`) as libwpd walks the document, and
 * exposes it to Rust through a flat C API returning owned UTF-8 that the Rust
 * side frees. Text and Markdown are two renderings of that one internal
 * document, produced only at the end, not two different things recorded
 * during the walk.
 *
 * Every entry point catches all C++ exceptions: libwpd throws on malformed
 * input, and an exception must never unwind across the FFI boundary.
 ~keep */
#include <librevenge-stream/librevenge-stream.h>
#include <librevenge/librevenge.h>
#include <libwpd/libwpd.h>

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <string>
#include <vector>

namespace {
using librevenge::RVNGPropertyList;
using librevenge::RVNGString;

/* One recorded event from the libwpd/librevenge callback walk. The document
 * is a flat `std::vector<Node>`; rendering (see `render` below) is the only
 * place that knows about output formats. `text`/`text2` and
 * `level`/`counter`/`counter2` are reused across kinds rather than giving
 * every kind its own dedicated fields (a link's href, a field's placeholder
 * kind, a metadata key/value pair, and a table cell's column/span all borrow
 * the same slots); each kind's comment below says what it puts there. ~keep */
enum class NodeKind {
    Text,
    Tab,
    Space,
    LineBreak,
    ParagraphEnd,
    ListItemEnd,
    Heading,
    BoldStart,
    BoldEnd,
    ItalicStart,
    ItalicEnd,
    UnderlineStart,
    UnderlineEnd,
    StrikethroughStart,
    StrikethroughEnd,
    SuperscriptStart,
    SuperscriptEnd,
    SubscriptStart,
    SubscriptEnd,
    ListItemStart,
    TableStart,
    TableRowStart,
    TableCellStart,
    CoveredTableCell,
    TableCellEnd,
    TableRowEnd,
    TableEnd,
    HeaderStart,
    HeaderEnd,
    FooterStart,
    FooterEnd,
    NoteStart,
    NoteEnd,
    EndnoteStart,
    EndnoteEnd,
    AsideStart,
    AsideEnd,
    LinkStart,
    LinkEnd,
    FieldInsert,
    MetaData,
};

struct Node {
    NodeKind kind;
    std::string text;     // literal text; link href; field placeholder; metadata key
    std::string text2;    // metadata value
    int level = 0;        // heading level; list nesting level; table cell column
    int counter = 0;      // ordered-list counter; table cell column span
    int counter2 = 0;     // table cell row span
    bool ordered = false; // list ordered flag; table row "is header row" flag
};

/* Records the document as a flat, format-agnostic `std::vector<Node>` while
 * libwpd walks it. Carries no notion of "plain text" vs "Markdown" — that
 * distinction exists only in `render`, which runs once, after the walk is
 * complete, over the recorded nodes. ~keep */
class DocumentBuilder : public librevenge::RVNGTextInterface {
  public:
    std::vector<Node> nodes;

    void insertText(const RVNGString &s) override {
        // `size()` is the byte length; `len()` is the UTF-8 *character* count,
        // which would both truncate multibyte text and stop at an embedded NUL. ~keep
        if (s.cstr())
            nodes.push_back({NodeKind::Text, std::string(s.cstr(), s.size())});
    }
    void insertTab() override {
        nodes.push_back({NodeKind::Tab});
    }
    void insertSpace() override {
        nodes.push_back({NodeKind::Space});
    }
    void insertLineBreak() override {
        nodes.push_back({NodeKind::LineBreak});
    }
    void closeParagraph() override {
        nodes.push_back({NodeKind::ParagraphEnd});
    }
    void closeListElement() override {
        nodes.push_back({NodeKind::ListItemEnd});
    }
    void closeTableCell() override {
        nodes.push_back({NodeKind::TableCellEnd});
    }
    void closeTableRow() override {
        nodes.push_back({NodeKind::TableRowEnd});
    }
    void closeTable() override {
        nodes.push_back({NodeKind::TableEnd});
    }

    void openParagraph(const RVNGPropertyList &props) override {
        const librevenge::RVNGProperty *outline = props["text:outline-level"];
        if (outline) {
            int level = outline->getInt();
            if (level >= 1 && level <= 6) {
                Node n{NodeKind::Heading};
                n.level = level;
                nodes.push_back(n);
            }
        }
    }

    void openSpan(const RVNGPropertyList &props) override {
        const librevenge::RVNGProperty *weight = props["fo:font-weight"];
        const librevenge::RVNGProperty *style = props["fo:font-style"];
        const librevenge::RVNGProperty *underline = props["style:text-underline-style"];
        const librevenge::RVNGProperty *lineThrough = props["fo:text-line-through-style"];
        const librevenge::RVNGProperty *position = props["style:text-position"];
        SpanFlags flags{};
        flags.bold = weight && weight->getStr() == "bold";
        flags.italic = style && style->getStr() == "italic";
        // libwpd emits "solid" for both single and double underline. ~keep
        flags.underline = underline && underline->getStr() != "none";
        flags.strikethrough = lineThrough && lineThrough->getStr() != "none";
        // libwpd emits "super <pct>%" / "sub <pct>%" (WPXContentListener). ~keep
        if (position) {
            std::string pos = position->getStr().cstr() ? position->getStr().cstr() : "";
            flags.superscript = pos.rfind("super", 0) == 0;
            flags.subscript = pos.rfind("sub", 0) == 0;
        }
        if (flags.bold)
            nodes.push_back({NodeKind::BoldStart});
        if (flags.italic)
            nodes.push_back({NodeKind::ItalicStart});
        if (flags.underline)
            nodes.push_back({NodeKind::UnderlineStart});
        if (flags.strikethrough)
            nodes.push_back({NodeKind::StrikethroughStart});
        if (flags.superscript)
            nodes.push_back({NodeKind::SuperscriptStart});
        if (flags.subscript)
            nodes.push_back({NodeKind::SubscriptStart});
        spanStack_.push_back(flags);
    }
    void closeSpan() override {
        if (spanStack_.empty())
            return;
        SpanFlags flags = spanStack_.back();
        spanStack_.pop_back();
        if (flags.subscript)
            nodes.push_back({NodeKind::SubscriptEnd});
        if (flags.superscript)
            nodes.push_back({NodeKind::SuperscriptEnd});
        if (flags.strikethrough)
            nodes.push_back({NodeKind::StrikethroughEnd});
        if (flags.underline)
            nodes.push_back({NodeKind::UnderlineEnd});
        if (flags.italic)
            nodes.push_back({NodeKind::ItalicEnd});
        if (flags.bold)
            nodes.push_back({NodeKind::BoldEnd});
    }

    void openOrderedListLevel(const RVNGPropertyList &) override {
        listStack_.push_back({true, 0});
    }
    void openUnorderedListLevel(const RVNGPropertyList &) override {
        listStack_.push_back({false, 0});
    }
    void closeOrderedListLevel() override {
        if (!listStack_.empty())
            listStack_.pop_back();
    }
    void closeUnorderedListLevel() override {
        if (!listStack_.empty())
            listStack_.pop_back();
    }
    void openListElement(const RVNGPropertyList &) override {
        if (listStack_.empty())
            return;
        ListLevel &level = listStack_.back();
        Node n{NodeKind::ListItemStart};
        n.level = static_cast<int>(listStack_.size());
        n.ordered = level.ordered;
        if (level.ordered) {
            level.counter += 1;
            n.counter = level.counter;
        }
        nodes.push_back(n);
    }

    // Headers and footers recur on every page rather than at one point in the
    // flow; rendering collects them once and exposes them at the start/end of
    // the document instead of splicing them inline (see `render`). ~keep
    void openHeader(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::HeaderStart});
    }
    void closeHeader() override {
        nodes.push_back({NodeKind::HeaderEnd});
    }
    void openFooter(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::FooterStart});
    }
    void closeFooter() override {
        nodes.push_back({NodeKind::FooterEnd});
    }

    // Footnotes, endnotes, comments and text boxes never belong inline in the
    // narrative. Notes are reference constructs, so rendering leaves a numbered
    // marker at the anchor and collects the bodies at the end of the document;
    // footnotes and endnotes are kept as distinct node kinds (rather than one
    // merged "note" kind) so `render` can number and label them as two
    // separate sequences instead of interleaving them under one counter;
    // comments and text boxes have no such numbering in the source and stay
    // bracketed where they occur (see `render`). ~keep
    void openFootnote(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::NoteStart});
    }
    void closeFootnote() override {
        nodes.push_back({NodeKind::NoteEnd});
    }
    void openEndnote(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::EndnoteStart});
    }
    void closeEndnote() override {
        nodes.push_back({NodeKind::EndnoteEnd});
    }
    void openComment(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::AsideStart, "comment"});
    }
    void closeComment() override {
        nodes.push_back({NodeKind::AsideEnd});
    }
    void openTextBox(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::AsideStart, "box"});
    }
    void closeTextBox() override {
        nodes.push_back({NodeKind::AsideEnd});
    }

    void openLink(const RVNGPropertyList &props) override {
        const librevenge::RVNGProperty *href = props["xlink:href"];
        Node n{NodeKind::LinkStart};
        if (href && href->getStr().cstr())
            n.text = href->getStr().cstr();
        nodes.push_back(n);
    }
    void closeLink() override {
        nodes.push_back({NodeKind::LinkEnd});
    }

    void insertField(const RVNGPropertyList &props) override {
        const librevenge::RVNGProperty *type = props["librevenge:field-type"];
        std::string fieldType = type && type->getStr().cstr() ? type->getStr().cstr() : "";
        Node n{NodeKind::FieldInsert};
        // A dropped field silently loses information a reader can't recover
        // (a page number that never appears anywhere in body text); render an
        // explicit placeholder instead so the field's presence, at least,
        // survives extraction. ~keep
        if (fieldType == "text:page-number")
            n.text = "page";
        else if (fieldType == "text:page-count")
            n.text = "pages";
        else if (fieldType.rfind("text:date", 0) == 0)
            n.text = "date";
        else if (fieldType.rfind("text:time", 0) == 0)
            n.text = "time";
        else if (!fieldType.empty())
            n.text = fieldType;
        else
            n.text = "field";
        nodes.push_back(n);
    }

    // setDocumentMetaData is always the first callback libwpd makes, so these
    // nodes land at the very front of `nodes` regardless of when they're
    // rendered. Only a handful of the keys RVNGTextInterface documents are
    // captured (plus dc:title, which some libwpd versions emit despite not
    // being in that list) — enough to round-trip the common case (title,
    // author, subject, keywords) without building a full structured metadata
    // API on the Rust side, which would be a much larger change to the FFI
    // surface for headers this extractor otherwise never inspects. ~keep
    void setDocumentMetaData(const RVNGPropertyList &props) override {
        static const char *const kKeys[] = {
            "dc:title", "dc:creator", "dc:subject", "dc:type", "dc:language", "meta:keyword",
        };
        for (const char *key : kKeys) {
            const librevenge::RVNGProperty *value = props[key];
            if (!value || !value->getStr().cstr())
                continue;
            Node n{NodeKind::MetaData};
            n.text = key;
            n.text2 = value->getStr().cstr();
            nodes.push_back(n);
        }
    }
    void startDocument(const RVNGPropertyList &) override {}
    void endDocument() override {}
    void definePageStyle(const RVNGPropertyList &) override {}
    void defineEmbeddedFont(const RVNGPropertyList &) override {}
    void openPageSpan(const RVNGPropertyList &) override {}
    void closePageSpan() override {}
    void defineParagraphStyle(const RVNGPropertyList &) override {}
    void defineCharacterStyle(const RVNGPropertyList &) override {}
    void defineSectionStyle(const RVNGPropertyList &) override {}
    void openSection(const RVNGPropertyList &) override {}
    void closeSection() override {}

    // Table structure is recorded fully (open events too, not just the
    // close-event markers the previous implementation emitted) so `render`
    // can lay cells out on a real grid: column, column span, row span and
    // whether a row is a header row. ~keep
    void openTable(const RVNGPropertyList &) override {
        nodes.push_back({NodeKind::TableStart});
    }
    void openTableRow(const RVNGPropertyList &props) override {
        const librevenge::RVNGProperty *header = props["librevenge:is-header-row"];
        Node n{NodeKind::TableRowStart};
        n.ordered = header && header->getInt() != 0;
        nodes.push_back(n);
    }
    void openTableCell(const RVNGPropertyList &props) override {
        Node n{NodeKind::TableCellStart};
        n.level = getIntOr(props, "librevenge:column", -1);
        n.counter = std::max(1, getIntOr(props, "table:number-columns-spanned", 1));
        n.counter2 = std::max(1, getIntOr(props, "table:number-rows-spanned", 1));
        nodes.push_back(n);
    }
    void insertCoveredTableCell(const RVNGPropertyList &props) override {
        Node n{NodeKind::CoveredTableCell};
        n.level = getIntOr(props, "librevenge:column", -1);
        nodes.push_back(n);
    }

    void openFrame(const RVNGPropertyList &) override {}
    void closeFrame() override {}
    void insertBinaryObject(const RVNGPropertyList &) override {}
    void insertEquation(const RVNGPropertyList &) override {}
    void openGroup(const RVNGPropertyList &) override {}
    void closeGroup() override {}
    void defineGraphicStyle(const RVNGPropertyList &) override {}
    void drawRectangle(const RVNGPropertyList &) override {}
    void drawEllipse(const RVNGPropertyList &) override {}
    void drawPolygon(const RVNGPropertyList &) override {}
    void drawPolyline(const RVNGPropertyList &) override {}
    void drawPath(const RVNGPropertyList &) override {}
    void drawConnector(const RVNGPropertyList &) override {}

  private:
    struct SpanFlags {
        bool bold;
        bool italic;
        bool underline;
        bool strikethrough;
        bool superscript;
        bool subscript;
    };
    struct ListLevel {
        bool ordered;
        int counter;
    };

    static int getIntOr(const RVNGPropertyList &props, const char *key, int fallback) {
        const librevenge::RVNGProperty *p = props[key];
        return p ? p->getInt() : fallback;
    }

    std::vector<SpanFlags> spanStack_;
    std::vector<ListLevel> listStack_;
};

/* Renders a recorded `std::vector<Node>` to text (`markdown = false`) or to
 * lightly Markdown-marked-up text (`markdown = true`). This is the only place
 * that knows about output formats — `DocumentBuilder` above records the same
 * structure regardless of which rendering will eventually be requested.
 *
 * Handles header/footer/aside placement identically in both modes: each is
 * accumulated into its own buffer via a sink stack and spliced back in
 * (headers/footers once, at the start/end; asides inline, bracketed) rather
 * than left to bleed into the surrounding narrative text. Tables are laid out
 * on a real grid built from the recorded column/span/header-row metadata: in
 * Markdown mode as GitHub-flavored pipe tables, in text mode as a tab/newline
 * grid with cell text sanitized so embedded tabs/newlines can't be mistaken
 * for cell or row boundaries. ~keep */
/* libwpd emits one flat span per formatting run, so a bold word inside an
 * italic sentence arrives as three consecutive runs rather than one nested
 * pair. Rendered naively that produces `***Bold**** rest*`, whose delimiter
 * runs are ambiguous to Markdown parsers. Closing a run and immediately
 * reopening the same one is a no-op, so drop both halves. ~keep */
bool isMatchingReopen(NodeKind end, NodeKind start) {
    switch (end) {
    case NodeKind::BoldEnd:
        return start == NodeKind::BoldStart;
    case NodeKind::ItalicEnd:
        return start == NodeKind::ItalicStart;
    case NodeKind::UnderlineEnd:
        return start == NodeKind::UnderlineStart;
    case NodeKind::StrikethroughEnd:
        return start == NodeKind::StrikethroughStart;
    case NodeKind::SuperscriptEnd:
        return start == NodeKind::SuperscriptStart;
    case NodeKind::SubscriptEnd:
        return start == NodeKind::SubscriptStart;
    default:
        return false;
    }
}

std::vector<Node> coalesceSpans(const std::vector<Node> &nodes) {
    std::vector<Node> out;
    out.reserve(nodes.size());
    for (const Node &n : nodes) {
        if (!out.empty() && isMatchingReopen(out.back().kind, n.kind)) {
            out.pop_back();
            continue;
        }
        out.push_back(n);
    }
    return out;
}

/* One table cell as recorded: `text` is already sanitized (see
 * `sanitizeCellText`) by the time it lands here. A covered (merged-away)
 * cell is recorded as an empty cell with span 1 so it still occupies a grid
 * position. ~keep */
struct CellRecord {
    std::string text;
    int colSpan = 1;
    // Absolute start column from librevenge:column. libwpd sets this on every
    // real cell (WPXContentListener::_openTableCell); covered cells and any
    // filler emitted on an error path carry -1 (unknown) and simply advance the
    // cursor by one. ~keep
    int column = -1;
};
struct RowRecord {
    std::vector<CellRecord> cells;
    bool isHeader = false;
};
struct TableRecord {
    std::vector<RowRecord> rows;
};

// Embedded tabs/newlines inside cell text would otherwise be indistinguishable
// from the tab/newline delimiters `render` itself uses for cell and row
// boundaries (text mode) or would break a Markdown pipe table's one-line-per-row
// syntax (markdown mode); both are neutralized here rather than left for the
// consumer to disambiguate. `|` is escaped only in Markdown mode, where it is
// the pipe-table column delimiter. ~keep
std::string sanitizeCellText(const std::string &raw, bool markdown) {
    std::string out;
    out.reserve(raw.size());
    for (char c : raw) {
        if (c == '\n') {
            out += markdown ? "<br>" : " ";
        } else if (c == '\t') {
            out += ' ';
        } else if (markdown && c == '|') {
            out += "\\|";
        } else {
            out += c;
        }
    }
    return out;
}

// Number of grid columns a row occupies, anchoring each real cell at its true
// librevenge:column and advancing by its span. Re-anchoring (rather than pure
// left-to-right accumulation) self-corrects the drift libwpd itself warns about
// for vertical merges ("insert covered cells with proper attributes" FIXME):
// even if a covered cell is dropped or duplicated, the next real cell snaps back
// to its declared column. ~keep
size_t rowWidth(const RowRecord &row) {
    size_t cursor = 0;
    size_t width = 0;
    for (const CellRecord &cell : row.cells) {
        if (cell.column >= 0 && static_cast<size_t>(cell.column) > cursor)
            cursor = static_cast<size_t>(cell.column);
        cursor += static_cast<size_t>(std::max(1, cell.colSpan));
        width = std::max(width, cursor);
    }
    return width;
}

std::vector<std::string> expandRow(const RowRecord &row, size_t columnCount) {
    std::vector<std::string> cols(columnCount);
    size_t cursor = 0;
    for (const CellRecord &cell : row.cells) {
        if (cell.column >= 0 && static_cast<size_t>(cell.column) > cursor)
            cursor = static_cast<size_t>(cell.column);
        if (cursor < columnCount)
            cols[cursor] = cell.text;
        cursor += static_cast<size_t>(std::max(1, cell.colSpan));
    }
    return cols;
}

size_t tableColumnCount(const TableRecord &table) {
    size_t columnCount = 1;
    for (const RowRecord &row : table.rows)
        columnCount = std::max(columnCount, rowWidth(row));
    return columnCount;
}

std::string renderTableMarkdown(const TableRecord &table) {
    if (table.rows.empty())
        return std::string();
    const size_t columnCount = tableColumnCount(table);

    bool hasHeaderRow = std::any_of(table.rows.begin(), table.rows.end(),
                                    [](const RowRecord &r) { return r.isHeader; });

    std::string out = "\n";
    std::string separator = "|";
    for (size_t i = 0; i < columnCount; ++i)
        separator += " --- |";
    separator += "\n";

    bool separatorEmitted = false;
    for (size_t i = 0; i < table.rows.size(); ++i) {
        const std::vector<std::string> cols = expandRow(table.rows[i], columnCount);
        out += "|";
        for (const std::string &c : cols) {
            out += ' ';
            out += c;
            out += " |";
        }
        out += "\n";
        // A table with no row explicitly flagged as a header still needs a
        // separator row to be valid Markdown; treating the first row as the
        // header in that case is a heuristic, not a fact recovered from the
        // source document. ~keep
        bool isHeaderBoundary = table.rows[i].isHeader || (!hasHeaderRow && i == 0);
        if (isHeaderBoundary && !separatorEmitted) {
            out += separator;
            separatorEmitted = true;
        }
    }
    return out;
}

std::string renderTableText(const TableRecord &table) {
    const size_t columnCount = tableColumnCount(table);
    std::string out;
    for (const RowRecord &row : table.rows) {
        const std::vector<std::string> cols = expandRow(row, columnCount);
        for (size_t i = 0; i < cols.size(); ++i) {
            if (i)
                out += '\t';
            out += cols[i];
        }
        out += '\n';
    }
    return out;
}

std::string render(const std::vector<Node> &rawNodes, bool markdown) {
    const std::vector<Node> nodes = coalesceSpans(rawNodes);
    std::string body;
    std::string header;
    std::string footer;
    std::string *sink = &body;
    std::vector<std::string *> sinkStack;
    std::vector<std::string> asideStack;
    std::vector<std::string> asideLabels;
    std::vector<std::string> footnotes;
    std::vector<std::string> footnoteStack;
    std::vector<std::string> endnotes;
    std::vector<std::string> endnoteStack;

    std::vector<TableRecord> tableStack;
    std::vector<RowRecord> rowStack;
    std::vector<std::string> cellStack;
    std::vector<std::pair<int, int>> cellSpanStack; // (colSpan, column) per open cell
    std::vector<std::string> linkHrefStack;

    auto pushSink = [&](std::string *s) {
        sinkStack.push_back(sink);
        sink = s;
    };
    auto popSink = [&]() {
        if (!sinkStack.empty()) {
            sink = sinkStack.back();
            sinkStack.pop_back();
        }
    };
    // Emphasis delimiters must hug the text: `**bold ** ` is not bold in any
    // Markdown dialect, so trailing run-internal spaces move outside the
    // closing delimiter (and leading ones outside the opening delimiter).
    // Document text is data, not markup: a literal '*' or '#' from the source
    // would otherwise be read as emphasis or a heading on the way out. ~keep
    auto appendText = [&](const std::string &text) {
        if (!markdown) {
            *sink += text;
            return;
        }
        for (char c : text) {
            if (std::strchr("\\`*_[]#$", c) != nullptr)
                *sink += '\\';
            *sink += c;
        }
    };
    struct OpenMark {
        std::string *sink;
        size_t end;
    };
    std::vector<OpenMark> openMarks;

    auto openEmphasis = [&](const char *delim) {
        if (!markdown)
            return;
        *sink += delim;
        openMarks.push_back({sink, sink->size()});
    };
    auto closeEmphasis = [&](const char *delim, size_t openLen) {
        if (!markdown)
            return;
        std::string spill;
        while (!sink->empty() && (sink->back() == ' ' || sink->back() == '\t')) {
            spill.insert(spill.begin(), sink->back());
            sink->pop_back();
        }
        // An emphasis run that captured no text is noise (`****`) and, worse,
        // can pair with a neighbouring run and swallow real text between them. ~keep
        if (!openMarks.empty()) {
            OpenMark mark = openMarks.back();
            openMarks.pop_back();
            if (mark.sink == sink && sink->size() == mark.end && sink->size() >= openLen) {
                sink->erase(sink->size() - openLen);
                *sink += spill;
                return;
            }
        }
        *sink += delim;
        *sink += spill;
    };

    std::vector<Node> metadata;

    for (const Node &n : nodes) {
        switch (n.kind) {
        case NodeKind::Text:
            appendText(n.text);
            break;
        case NodeKind::Tab:
            *sink += '\t';
            break;
        case NodeKind::Space:
            *sink += ' ';
            break;
        case NodeKind::LineBreak:
            *sink += '\n';
            break;
        case NodeKind::ParagraphEnd:
            *sink += "\n\n";
            break;
        case NodeKind::ListItemEnd:
            *sink += '\n';
            break;
        case NodeKind::Heading:
            if (markdown)
                *sink += std::string(static_cast<size_t>(n.level), '#') + ' ';
            break;
        case NodeKind::BoldStart:
            openEmphasis("**");
            break;
        case NodeKind::BoldEnd:
            closeEmphasis("**", 2);
            break;
        case NodeKind::ItalicStart:
            openEmphasis("*");
            break;
        case NodeKind::ItalicEnd:
            closeEmphasis("*", 1);
            break;
        // Markdown has no underline, strikethrough, superscript or subscript
        // syntax of its own; strikethrough uses the GFM `~~` convention and
        // the rest fall back to the inline HTML CommonMark leaves for them. ~keep
        case NodeKind::UnderlineStart:
            openEmphasis("<u>");
            break;
        case NodeKind::UnderlineEnd:
            closeEmphasis("</u>", 3);
            break;
        case NodeKind::StrikethroughStart:
            openEmphasis("~~");
            break;
        case NodeKind::StrikethroughEnd:
            closeEmphasis("~~", 2);
            break;
        case NodeKind::SuperscriptStart:
            openEmphasis("<sup>");
            break;
        case NodeKind::SuperscriptEnd:
            closeEmphasis("</sup>", 5);
            break;
        case NodeKind::SubscriptStart:
            openEmphasis("<sub>");
            break;
        case NodeKind::SubscriptEnd:
            closeEmphasis("</sub>", 5);
            break;
        case NodeKind::ListItemStart:
            if (markdown) {
                std::string indent(static_cast<size_t>(n.level - 1) * 2, ' ');
                *sink += n.ordered ? indent + std::to_string(n.counter) + ". " : indent + "- ";
            }
            break;
        case NodeKind::LinkStart:
            if (markdown)
                *sink += '[';
            linkHrefStack.push_back(n.text);
            break;
        case NodeKind::LinkEnd:
            if (!linkHrefStack.empty()) {
                std::string href = std::move(linkHrefStack.back());
                linkHrefStack.pop_back();
                if (markdown)
                    *sink += "](" + href + ")";
                else if (!href.empty())
                    *sink += " (" + href + ")";
            }
            break;
        case NodeKind::FieldInsert:
            *sink += "[" + n.text + "]";
            break;
        case NodeKind::MetaData:
            metadata.push_back(n);
            break;
        case NodeKind::TableStart:
            tableStack.push_back(TableRecord{});
            rowStack.push_back(RowRecord{});
            break;
        case NodeKind::TableRowStart:
            rowStack.back() = RowRecord{};
            rowStack.back().isHeader = n.ordered;
            break;
        case NodeKind::TableCellStart:
            cellSpanStack.push_back({std::max(1, n.counter), n.level});
            cellStack.push_back(std::string());
            pushSink(&cellStack.back());
            break;
        case NodeKind::TableCellEnd: {
            if (cellStack.empty() || rowStack.empty())
                break;
            std::string text = sanitizeCellText(cellStack.back(), markdown);
            cellStack.pop_back();
            popSink();
            int colSpan = 1;
            int column = -1;
            if (!cellSpanStack.empty()) {
                colSpan = cellSpanStack.back().first;
                column = cellSpanStack.back().second;
                cellSpanStack.pop_back();
            }
            rowStack.back().cells.push_back({text, colSpan, column});
            break;
        }
        case NodeKind::CoveredTableCell:
            if (!rowStack.empty())
                rowStack.back().cells.push_back({std::string(), 1, -1});
            break;
        case NodeKind::TableRowEnd:
            if (!tableStack.empty() && !rowStack.empty())
                tableStack.back().rows.push_back(rowStack.back());
            break;
        case NodeKind::TableEnd:
            if (!tableStack.empty()) {
                TableRecord table = std::move(tableStack.back());
                tableStack.pop_back();
                if (!rowStack.empty())
                    rowStack.pop_back();
                *sink += markdown ? renderTableMarkdown(table) : renderTableText(table);
            }
            break;
        case NodeKind::HeaderStart:
            pushSink(&header);
            break;
        case NodeKind::HeaderEnd:
            popSink();
            break;
        case NodeKind::FooterStart:
            pushSink(&footer);
            break;
        case NodeKind::FooterEnd:
            popSink();
            break;
        case NodeKind::NoteStart:
            footnoteStack.push_back(std::string());
            pushSink(&footnoteStack.back());
            break;
        case NodeKind::NoteEnd: {
            if (footnoteStack.empty())
                break;
            std::string content = std::move(footnoteStack.back());
            footnoteStack.pop_back();
            popSink();
            while (!content.empty() && content.back() == '\n')
                content.pop_back();
            footnotes.push_back(std::move(content));
            std::string ref = std::to_string(footnotes.size());
            *sink += markdown ? "[^" + ref + "]" : "[" + ref + "]";
            break;
        }
        case NodeKind::EndnoteStart:
            endnoteStack.push_back(std::string());
            pushSink(&endnoteStack.back());
            break;
        case NodeKind::EndnoteEnd: {
            if (endnoteStack.empty())
                break;
            std::string content = std::move(endnoteStack.back());
            endnoteStack.pop_back();
            popSink();
            while (!content.empty() && content.back() == '\n')
                content.pop_back();
            endnotes.push_back(std::move(content));
            std::string ref = "e" + std::to_string(endnotes.size());
            *sink += markdown ? "[^" + ref + "]" : "[" + ref + "]";
            break;
        }
        case NodeKind::AsideStart:
            asideLabels.push_back(n.text);
            asideStack.push_back(std::string());
            pushSink(&asideStack.back());
            break;
        case NodeKind::AsideEnd: {
            if (asideStack.empty())
                break;
            std::string content = std::move(asideStack.back());
            asideStack.pop_back();
            std::string label = std::move(asideLabels.back());
            asideLabels.pop_back();
            popSink();
            while (!content.empty() && content.back() == '\n')
                content.pop_back();
            *sink += "\n[" + label + ": " + content + "]\n";
            break;
        }
        }
    }

    std::string out;
    // Metadata gets a minimal YAML-ish front-matter block, Markdown mode
    // only: this is a lighter-weight choice than adding a second, structured
    // return path to the C API purely to carry a handful of summary fields
    // (see the comment on `setDocumentMetaData`). Text mode leaves metadata
    // out entirely rather than inventing a plain-text convention for it. ~keep
    if (markdown && !metadata.empty()) {
        out += "---\n";
        for (const Node &m : metadata)
            out += m.text + ": \"" + m.text2 + "\"\n";
        out += "---\n\n";
    }
    if (!header.empty())
        out += "[header: " + header + "]\n\n";
    out += body;
    if (!footnotes.empty()) {
        while (!out.empty() && out.back() == '\n')
            out.pop_back();
        for (size_t i = 0; i < footnotes.size(); ++i) {
            std::string ref = std::to_string(i + 1);
            out += markdown ? "\n\n[^" + ref + "]: " + footnotes[i]
                            : "\n\n[" + ref + "] " + footnotes[i];
        }
    }
    if (!endnotes.empty()) {
        while (!out.empty() && out.back() == '\n')
            out.pop_back();
        for (size_t i = 0; i < endnotes.size(); ++i) {
            std::string ref = "e" + std::to_string(i + 1);
            out += markdown ? "\n\n[^" + ref + "]: " + endnotes[i]
                            : "\n\n[" + ref + "] " + endnotes[i];
        }
    }
    if (!footer.empty())
        out += "\n\n[footer: " + footer + "]";
    return out;
}
}

extern "C" {

/* Result codes shared with the Rust side (see error.rs). ~keep */
enum {
    XBERG_WPD_OK = 0,
    XBERG_WPD_INVALID_ARGS = 1,
    XBERG_WPD_UNSUPPORTED_FORMAT = 2,
    XBERG_WPD_PARSE_ERROR = 3,
    XBERG_WPD_OUT_OF_MEMORY = 4,
    XBERG_WPD_PANIC = 5,
    XBERG_WPD_ENCRYPTED = 6,
};

namespace {
char *dup_malloc(const char *data, size_t n) {
    char *buf = static_cast<char *>(std::malloc(n + 1));
    if (!buf)
        return nullptr;
    if (n)
        std::memcpy(buf, data, n);
    buf[n] = '\0';
    return buf;
}
}

/* Returns non-zero if the buffer looks like a WordPerfect document libwpd can
 * parse. Never throws. ~keep */
int xberg_wpd_is_supported(const unsigned char *data, unsigned long len) {
    if (!data || len == 0)
        return 0;
    if (len > (std::numeric_limits<unsigned int>::max)())
        return 0;
    try {
        librevenge::RVNGStringStream input(data, static_cast<unsigned int>(len));
        return libwpd::WPDocument::isFileFormatSupported(&input) != libwpd::WPD_CONFIDENCE_NONE ? 1
                                                                                                : 0;
    } catch (...) {
        return 0;
    }
}

/* Extract text (or, if `markdown` is non-zero, lightly Markdown-marked-up
 * text) from an in-memory WordPerfect document. Parses once into an internal
 * `std::vector<Node>` document via `DocumentBuilder`, then renders that one
 * document to the requested format — the two output modes are two renderings
 * of the same recorded structure, not two different things produced during
 * the libwpd walk.
 *
 * On XBERG_WPD_OK, *out_text is a malloc'd buffer of *out_len bytes (NOT
 * necessarily NUL-terminated at that length if the document contained an
 * embedded NUL; a trailing NUL is appended anyway for defensive C-string use
 * but callers must use *out_len as the authoritative length) the caller frees
 * via xberg_wpd_free_string. On any other return, *out_text is left null.
 *
 * On failure, *out_err may be set to a malloc'd, NUL-terminated diagnostic
 * message (freed the same way) describing the underlying C++ exception; it
 * is left null when no additional detail is available. ~keep */
int xberg_wpd_extract(const unsigned char *data, unsigned long len, int markdown, char **out_text,
                      unsigned long *out_len, char **out_err) {
    if (!out_text || !out_len)
        return XBERG_WPD_INVALID_ARGS;
    *out_text = nullptr;
    *out_len = 0;
    if (out_err)
        *out_err = nullptr;
    if (!data || len == 0)
        return XBERG_WPD_INVALID_ARGS;
    // RVNGStringStream takes an unsigned int; the Rust wrapper already rejects
    // oversized buffers, but direct C callers reach this boundary too. ~keep
    if (len > (std::numeric_limits<unsigned int>::max)())
        return XBERG_WPD_INVALID_ARGS;

    try {
        librevenge::RVNGStringStream input(data, static_cast<unsigned int>(len));
        if (libwpd::WPDocument::isFileFormatSupported(&input) == libwpd::WPD_CONFIDENCE_NONE)
            return XBERG_WPD_UNSUPPORTED_FORMAT;

        DocumentBuilder builder;
        libwpd::WPDResult result = libwpd::WPDocument::parse(&input, &builder, nullptr);
        if (result != libwpd::WPD_OK) {
            // Encryption/password failures are distinguished from generic parse
            // errors so a caller can tell "this needs a password" from
            // "this file is corrupt" instead of both collapsing into the same
            // opaque error. ~keep
            if (result == libwpd::WPD_UNSUPPORTED_ENCRYPTION_ERROR ||
                result == libwpd::WPD_PASSWORD_MISSMATCH_ERROR)
                return XBERG_WPD_ENCRYPTED;
            return XBERG_WPD_PARSE_ERROR;
        }

        std::string rendered = render(builder.nodes, markdown != 0);
        if (rendered.size() > (std::numeric_limits<unsigned long>::max)())
            return XBERG_WPD_OUT_OF_MEMORY;
        char *buf = dup_malloc(rendered.data(), rendered.size());
        if (!buf)
            return XBERG_WPD_OUT_OF_MEMORY;
        *out_text = buf;
        *out_len = static_cast<unsigned long>(rendered.size());
        return XBERG_WPD_OK;
    } catch (const std::exception &e) {
        // libwpd's own error types (ParseException, FileException,
        // GenericException, ...) are bare classes that do not derive from
        // std::exception, so they land in the catch-all below; this arm covers
        // allocation and standard-library failures. what() may return null. ~keep
        const char *msg = e.what();
        if (out_err && msg)
            *out_err = dup_malloc(msg, std::strlen(msg));
        return XBERG_WPD_PANIC;
    } catch (...) {
        return XBERG_WPD_PANIC;
    }
}

void xberg_wpd_free_string(char *s) {
    std::free(s);
}

/* Internal self-test for the aside-separation logic in `render` (see its
 * comment above): drives `DocumentBuilder`'s callbacks directly, the same way
 * libwpd would, without needing a real WordPerfect document on disk. Exposed
 * so the Rust test suite has real evidence that footnote/header content is
 * bracketed apart from body text rather than concatenated into it. Not part
 * of the crate's public API contract. Returns non-zero on success. ~keep */
int xberg_wpd_self_test_separation(void) try {
    DocumentBuilder b;

    RVNGPropertyList empty;
    b.openHeader(empty);
    b.insertText(RVNGString("Confidential Draft"));
    b.closeHeader();

    b.openParagraph(empty);
    b.insertText(RVNGString("Body start."));
    b.openFootnote(empty);
    b.insertText(RVNGString("See appendix A."));
    b.closeFootnote();
    b.insertText(RVNGString("Body continues."));
    b.closeParagraph();

    b.openFooter(empty);
    b.insertText(RVNGString("Page 1 of 1"));
    b.closeFooter();

    std::string out = render(b.nodes, false);

    bool ok = true;
    ok = ok && out.find("[header: Confidential Draft]") != std::string::npos;
    ok = ok && out.find("[footer: Page 1 of 1]") != std::string::npos;
    ok = ok && out.find("[1] See appendix A.") != std::string::npos;
    ok = ok && out.find("Body start.[1]Body continues.") != std::string::npos;
    size_t anchor = out.find("[1]");
    size_t collected = out.find("[1] See appendix A.");
    ok = ok && anchor != std::string::npos && collected != std::string::npos && anchor < collected;
    ok = ok && out.find("See appendix A.") > out.find("Body continues.");
    // The header text must never appear anywhere but inside its own marker. ~keep
    size_t body_start = out.find("Body start.");
    ok = ok && body_start != std::string::npos &&
         out.find("Confidential Draft", body_start) == std::string::npos;

    return ok ? 1 : 0;
} catch (...) {
    return 0;
}

/* Internal self-test for the internal-document-model completeness work: link
 * hrefs, field placeholders, strikethrough spans, footnote/endnote
 * separation, table structure (header row, column span, a covered/merged
 * cell) and metadata front matter. Same rationale as
 * `xberg_wpd_self_test_separation` above: real evidence without needing a
 * WordPerfect fixture on disk for every feature. Returns non-zero on
 * success. ~keep */
int xberg_wpd_self_test_features(void) try {
    DocumentBuilder b;
    RVNGPropertyList empty;

    RVNGPropertyList meta;
    meta.insert("dc:title", "Sample Report");
    meta.insert("dc:creator", "A. Writer");
    b.setDocumentMetaData(meta);

    b.openParagraph(empty);
    RVNGPropertyList linkProps;
    linkProps.insert("xlink:href", "https://example.com/report");
    b.openLink(linkProps);
    b.insertText(RVNGString("full report"));
    b.closeLink();
    b.insertText(RVNGString(" "));

    RVNGPropertyList strikeProps;
    strikeProps.insert("fo:text-line-through-style", "solid");
    b.openSpan(strikeProps);
    b.insertText(RVNGString("obsolete"));
    b.closeSpan();
    b.insertText(RVNGString(" "));

    RVNGPropertyList pageFieldProps;
    pageFieldProps.insert("librevenge:field-type", "text:page-number");
    b.insertField(pageFieldProps);

    b.openFootnote(empty);
    b.insertText(RVNGString("A footnote."));
    b.closeFootnote();
    b.openEndnote(empty);
    b.insertText(RVNGString("An endnote."));
    b.closeEndnote();
    b.closeParagraph();

    b.openTable(empty);
    RVNGPropertyList headerRowProps;
    headerRowProps.insert("librevenge:is-header-row", true);
    b.openTableRow(headerRowProps);
    b.openTableCell(empty);
    b.insertText(RVNGString("Name"));
    b.closeTableCell();
    RVNGPropertyList spanCellProps;
    spanCellProps.insert("table:number-columns-spanned", 2);
    b.openTableCell(spanCellProps);
    b.insertText(RVNGString("Contact"));
    b.closeTableCell();
    b.closeTableRow();

    b.openTableRow(empty);
    b.openTableCell(empty);
    // Embedded tab/newline must not corrupt cell/row boundaries. ~keep
    b.insertText(RVNGString("Jo|e"));
    b.insertLineBreak();
    b.insertText(RVNGString("Doe"));
    b.closeTableCell();
    b.insertCoveredTableCell(empty);
    b.openTableCell(empty);
    b.insertText(RVNGString("jo@example.com"));
    b.closeTableCell();
    b.closeTableRow();
    b.closeTable();

    // A second table exercising column re-anchoring: libwpd documents that it
    // sometimes fails to emit a covered cell for a vertical merge ("this case
    // should not happen, but it happens in real-life documents"), yet it always
    // stamps the surviving real cell with its true librevenge:column. Here row 2
    // omits the column-0 covered cell but declares its cell at column 1; the
    // renderer must still place it in the second column, not the first. ~keep
    DocumentBuilder c;
    c.openTable(empty);
    c.openTableRow(empty);
    RVNGPropertyList r1c0;
    r1c0.insert("librevenge:column", 0);
    c.openTableCell(r1c0);
    c.insertText(RVNGString("r1c0"));
    c.closeTableCell();
    RVNGPropertyList r1c1;
    r1c1.insert("librevenge:column", 1);
    c.openTableCell(r1c1);
    c.insertText(RVNGString("r1c1"));
    c.closeTableCell();
    c.closeTableRow();
    c.openTableRow(empty);
    RVNGPropertyList r2c1;
    r2c1.insert("librevenge:column", 1);
    c.openTableCell(r2c1);
    c.insertText(RVNGString("r2c1"));
    c.closeTableCell();
    c.closeTableRow();
    c.closeTable();
    std::string reanchored = render(c.nodes, true);

    std::string md = render(b.nodes, true);
    std::string text = render(b.nodes, false);

    bool ok = true;
    ok = ok && md.find("---\ndc:title: \"Sample Report\"") != std::string::npos;
    ok = ok && md.find("[full report](https://example.com/report)") != std::string::npos;
    ok = ok && md.find("~~obsolete~~") != std::string::npos;
    ok = ok && md.find("[page]") != std::string::npos;
    ok = ok && md.find("[^1]") != std::string::npos &&
         md.find("[^1]: A footnote.") != std::string::npos;
    ok = ok && md.find("[^e1]") != std::string::npos &&
         md.find("[^e1]: An endnote.") != std::string::npos;
    ok = ok && md.find("| Name | Contact |  |") != std::string::npos;
    ok = ok && md.find("| --- | --- | --- |") != std::string::npos;
    ok = ok && md.find("Jo\\|e<br>Doe") != std::string::npos;
    // The embedded newline was folded into a space rather than left as a raw
    // '\n' that would otherwise be indistinguishable from a row boundary. ~keep
    ok = ok && text.find("Jo|e Doe\t\tjo@example.com") != std::string::npos;
    ok = ok && text.find("Name\tContact\t") != std::string::npos;
    // Re-anchoring: the dropped column-0 covered cell leaves an empty first
    // column and r2c1 lands in the second, matching row 1's real columns. ~keep
    ok = ok && reanchored.find("| r1c0 | r1c1 |") != std::string::npos;
    ok = ok && reanchored.find("|  | r2c1 |") != std::string::npos;

    return ok ? 1 : 0;
} catch (...) {
    return 0;
}
}
