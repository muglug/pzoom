<?php
class PluginY {}

/** @param non-empty-string $s */
function needsNonEmpty(string $s): string { return $s; }

function formats(string $free, int $n): void {
    needsNonEmpty(sprintf('<psalm plugin="%s"/>', PluginY::class));
    needsNonEmpty(sprintf('%d', $n));
    needsNonEmpty(sprintf('%s!', $free));
    needsNonEmpty(sprintf('%s', PluginY::class));
}
