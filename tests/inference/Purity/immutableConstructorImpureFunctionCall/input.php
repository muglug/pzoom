<?php

/** @psalm-immutable */
final class FileConfig {
    public string $primary;
    public string $secondary;

    public function __construct(string $a, string $b) {
        // Impure builtins are not allowed in the external-mutation-free
        // constructor context.
        $this->primary = file_get_contents($a) ?: '';
        $this->secondary = file_get_contents($b) ?: '';
    }
}
