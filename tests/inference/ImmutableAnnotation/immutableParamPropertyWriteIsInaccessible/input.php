<?php
/** @psalm-immutable */
final class Foo {
    public function __construct(public string $bar) {}
    public function merge(Foo $other): void {
        $other->bar = $this->bar;
    }
}
