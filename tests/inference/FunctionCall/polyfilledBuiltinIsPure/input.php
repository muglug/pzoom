<?php

namespace {
    if (!function_exists('mb_strlen2')) {
    }
}

namespace App {
    /** @psalm-immutable */
    class Lit {
        public function __construct(public string $value) {}

        public function getId(): string
        {
            if (mb_strlen($this->value) > 80) {
                return mb_substr($this->value, 0, 80) . '...';
            }
            return $this->value;
        }
    }
}
