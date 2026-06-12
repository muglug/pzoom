<?php

class Restarter {
    private const SETTINGS = [
        'jit_buffer_size' => 128 * 1024 * 1024,
        'interned_strings_buffer' => 64,
    ];

    private const PATTERN = <<<'EOF2'
    /x/
    EOF2;

    /** @return positive-int */
    public function requiredMemory(bool $jit): int
    {
        $result = 256;
        if ($jit) {
            $result += self::SETTINGS['jit_buffer_size'] / 1024 / 1024;
        }
        if (isset(self::SETTINGS['interned_strings_buffer'])) {
            $result += self::SETTINGS['interned_strings_buffer'];
        }
        return $result;
    }

    /** @return non-empty-string */
    public function pattern(): string
    {
        return self::PATTERN;
    }
}
