```typescript title="TypeScript"
import { extract } from '@xberg-io/xberg';

const output = await extract({ kind: "uri", uri: "ticket.pdf" }, { qrCodes: true });
for (const image of output.results[0].images ?? []) {
    for (const qr of image.qrCodes ?? []) {
        console.log(qr.payload);
    }
}
```
