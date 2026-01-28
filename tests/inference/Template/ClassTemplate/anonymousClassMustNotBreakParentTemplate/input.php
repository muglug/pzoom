<?php
/** @template T */
class Foo {
    /** @psalm-var ?T */
    private $value;

    /** @psalm-param T $val */
    public function set($val) : void {
        $this->value = $val;
        /** @psalm-suppress MissingTemplateParam */
        new class extends Foo {};
    }

    /** @psalm-return ?T */
    public function get() {
        return $this->value;
    }
}
