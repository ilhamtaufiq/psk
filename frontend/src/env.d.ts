/// <reference types="astro/client" />

declare namespace App {
  interface Locals {
    token: string | null;
    user?: import('./lib/api').User;
  }
}
