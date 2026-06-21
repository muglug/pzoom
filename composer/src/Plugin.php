<?php

declare(strict_types=1);

namespace Pzoom\Composer;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\ScriptEvents;

/**
 * Composer plugin that fetches the prebuilt native `pzoom` binary after the
 * package is installed or updated.
 *
 * A dependency's own `scripts` are never executed on the consumer's machine, so
 * a plugin subscribing to Composer's script events is the supported way to run
 * install-time logic from a library.
 */
final class Plugin implements PluginInterface, EventSubscriberInterface
{
    private Composer $composer;
    private IOInterface $io;

    public function activate(Composer $composer, IOInterface $io): void
    {
        $this->composer = $composer;
        $this->io = $io;
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
    }

    /**
     * @return array<string, string>
     */
    public static function getSubscribedEvents(): array
    {
        return [
            ScriptEvents::POST_INSTALL_CMD => 'installBinary',
            ScriptEvents::POST_UPDATE_CMD => 'installBinary',
        ];
    }

    public function installBinary(): void
    {
        (new BinaryInstaller($this->composer, $this->io))->install();
    }
}
