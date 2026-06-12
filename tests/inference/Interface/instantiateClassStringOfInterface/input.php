<?php
interface PluginInterface {}

function loadPlugin(string $pluginClassName): PluginInterface {
    if (!is_a($pluginClassName, PluginInterface::class, true)) {
        throw new UnexpectedValueException('not a plugin');
    }
    return new $pluginClassName;
}
