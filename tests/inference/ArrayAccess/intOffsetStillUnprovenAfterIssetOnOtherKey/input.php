<?php

final class Env {
    /** @var array */
    private array $values;

    public function __construct(array $values) {
        $this->values = $values;
    }

    public function read(): string {
        // isset() on some int keys narrows $this->values to a shape that keeps
        // the generic `array-key` fallback, so accessing a *different* int key
        // is still risky (Psalm's ensureArrayIntOffsetsExist).
        if (isset($this->values[1]) && isset($this->values[2])) {
            return (string) $this->values[3];
        }
        return '';
    }
}
