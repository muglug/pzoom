<?php

class TNamedObject2 extends Atomic2 {
    public string $value = '';
}

abstract class Atomic2 {
    /** @psalm-assert-if-true TNamedObject2 $this */
    public function isNamed(): bool
    {
        return $this instanceof TNamedObject2;
    }

    public function probe(): string
    {
        if ($this->isNamed()) {
            return strtolower($this->value);
        }
        return '';
    }
}
