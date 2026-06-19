<?php

final class Env {
    /** @var array */
    private array $values;

    public function __construct(array $values) {
        $this->values = $values;
    }

    public function read(): string {
        // isset() on some keys narrows $this->values to a shape that keeps the
        // generic `array-key` fallback, so accessing a *different* string key is
        // still risky (Psalm's ensureArrayStringOffsetsExist).
        if (isset($this->values['HOST']) && isset($this->values['PORT'])) {
            return (string) $this->values['SCHEME'];
        }
        return '';
    }
}
