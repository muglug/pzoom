<?php

declare(strict_types=1);

namespace Pzoom\Composer;

use Composer\Composer;
use Composer\IO\IOInterface;
use Composer\Package\PackageInterface;
use Composer\Util\HttpDownloader;
use Throwable;

/**
 * Downloads the prebuilt native `pzoom` binary that matches the installed
 * package version and the host platform, and places it next to the `bin/pzoom`
 * launcher.
 */
final class BinaryInstaller
{
    private const PACKAGE_NAME = 'muglug/pzoom';
    private const RELEASE_BASE = 'https://github.com/muglug/pzoom/releases/download/';

    private Composer $composer;
    private IOInterface $io;

    public function __construct(Composer $composer, IOInterface $io)
    {
        $this->composer = $composer;
        $this->io = $io;
    }

    public function install(): void
    {
        // When developing pzoom itself the package is the root package; there is
        // nothing to download.
        if ($this->composer->getPackage()->getName() === self::PACKAGE_NAME) {
            return;
        }

        $package = $this->findPackage();
        if ($package === null) {
            return;
        }

        $target = $this->resolveTarget();
        if ($target === null) {
            $this->io->warning(sprintf(
                'pzoom: no prebuilt binary is available for %s/%s. Build from source: https://github.com/muglug/pzoom#building',
                PHP_OS_FAMILY,
                php_uname('m')
            ));
            return;
        }

        $tag = $this->resolveTag($package);
        if ($tag === null) {
            return;
        }

        $installPath = $this->composer->getInstallationManager()->getInstallPath($package);
        $binDir = rtrim((string) $installPath, '/\\') . DIRECTORY_SEPARATOR . 'bin';
        $destination = $binDir . DIRECTORY_SEPARATOR . 'pzoom-native';
        $marker = $binDir . DIRECTORY_SEPARATOR . '.pzoom-binary-version';

        // Skip if the correct binary is already present.
        if (is_file($destination) && @file_get_contents($marker) === $tag) {
            return;
        }

        $asset = 'pzoom-' . $target;
        $url = $this->releaseBase() . rawurlencode($tag) . '/' . $asset;

        if (!is_dir($binDir) && !@mkdir($binDir, 0777, true) && !is_dir($binDir)) {
            $this->io->writeError("<error>pzoom: could not create {$binDir}</error>");
            return;
        }

        $httpDownloader = new HttpDownloader($this->io, $this->composer->getConfig());

        $this->io->write("  - Fetching <info>pzoom</info> {$tag} ({$target})");

        try {
            $body = $httpDownloader->get($url)->getBody();
        } catch (Throwable $e) {
            $this->io->writeError("<error>pzoom: failed to download {$url}: {$e->getMessage()}</error>");
            $this->io->writeError('<error>pzoom: ensure a release exists for this version with binaries for your platform.</error>');
            return;
        }

        if (!is_string($body) || $body === '') {
            $this->io->writeError("<error>pzoom: downloaded an empty file from {$url}</error>");
            return;
        }

        if (!$this->checksumMatches($httpDownloader, $url, $body, $asset)) {
            return;
        }

        $temporary = $destination . '.download';
        if (@file_put_contents($temporary, $body) === false || !@rename($temporary, $destination)) {
            @unlink($temporary);
            $this->io->writeError("<error>pzoom: could not install binary to {$destination}</error>");
            return;
        }

        @chmod($destination, 0755);
        @file_put_contents($marker, $tag);

        $this->io->write('  - Installed <info>pzoom</info> binary');
    }

    /**
     * Base URL that release assets are downloaded from, with a trailing slash.
     * Overridable for mirrors or air-gapped installs.
     */
    private function releaseBase(): string
    {
        $override = getenv('PZOOM_RELEASE_BASE');
        if (is_string($override) && $override !== '') {
            return rtrim($override, '/') . '/';
        }

        return self::RELEASE_BASE;
    }

    private function findPackage(): ?PackageInterface
    {
        $repository = $this->composer->getRepositoryManager()->getLocalRepository();
        foreach ($repository->getPackages() as $package) {
            if ($package->getName() === self::PACKAGE_NAME) {
                return $package;
            }
        }

        return null;
    }

    /**
     * Determine which release tag to download from.
     *
     * Binaries ship with tagged `vX.Y.Z` releases, so a tagged install maps
     * directly to a tag. An explicit override (env var or `extra.pzoom.tag`)
     * wins, which is also how a dev install can opt into a specific release.
     */
    private function resolveTag(PackageInterface $package): ?string
    {
        $override = getenv('PZOOM_VERSION');
        if (is_string($override) && $override !== '') {
            return $override;
        }

        $extra = $package->getExtra();
        if (isset($extra['pzoom']['tag']) && is_string($extra['pzoom']['tag']) && $extra['pzoom']['tag'] !== '') {
            return $extra['pzoom']['tag'];
        }

        $version = $package->getPrettyVersion();
        if (str_starts_with($version, 'dev-') || str_ends_with($version, '-dev')) {
            $this->io->warning(
                'pzoom: a development version is installed, which has no associated binary release. '
                . 'Set the PZOOM_VERSION environment variable to a release tag (e.g. v0.1.0) to fetch a binary.'
            );
            return null;
        }

        return 'v' . ltrim($version, 'v');
    }

    /**
     * Map the host OS/architecture to a Rust target triple matching a release
     * asset name. Returns null when no prebuilt binary is published.
     */
    private function resolveTarget(): ?string
    {
        $arch = match (strtolower(php_uname('m'))) {
            'x86_64', 'amd64' => 'x86_64',
            'arm64', 'aarch64' => 'aarch64',
            default => null,
        };

        return match (true) {
            PHP_OS_FAMILY === 'Linux' && $arch === 'x86_64' => 'x86_64-unknown-linux-gnu',
            PHP_OS_FAMILY === 'Linux' && $arch === 'aarch64' => 'aarch64-unknown-linux-gnu',
            PHP_OS_FAMILY === 'Darwin' && $arch === 'aarch64' => 'aarch64-apple-darwin',
            default => null,
        };
    }

    private function checksumMatches(HttpDownloader $httpDownloader, string $url, string $contents, string $asset): bool
    {
        try {
            $body = $httpDownloader->get($url . '.sha256')->getBody();
        } catch (Throwable $e) {
            $this->io->warning("pzoom: could not fetch checksum for {$asset}; skipping verification ({$e->getMessage()})");
            return true;
        }

        if (!is_string($body) || trim($body) === '') {
            return true;
        }

        $expected = strtolower((string) strtok(trim($body), " \t\n"));

        if (!hash_equals($expected, hash('sha256', $contents))) {
            $this->io->writeError("<error>pzoom: checksum mismatch for {$asset}; refusing to install.</error>");
            return false;
        }

        return true;
    }
}
