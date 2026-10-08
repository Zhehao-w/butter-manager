import lucideLicense from './assets/lucide/LICENSE?raw';
import featherLicense from './assets/lucide/FEATHER-LICENSE?raw';
import tanstackLicense from './assets/third-party/tanstack-virtual-LICENSE.txt?raw';

// Pack the required upstream notices with the desktop app as well as keeping source copies.
export const iconLicense = `Lucide 0.468.0 — https://lucide.dev\n\n${lucideLicense}\n\nFeather-derived icons\n\n${featherLicense}`;
export const virtualLicense = `@tanstack/react-virtual 3.14.13 / @tanstack/virtual-core 3.17.11\nhttps://tanstack.com/virtual\n\n${tanstackLicense}`;
