/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // The desktop app + this site both have lockfiles; pin tracing to this folder
  // so Vercel (Root Directory = site/) bundles the right files.
  outputFileTracingRoot: import.meta.dirname,
};

export default nextConfig;
