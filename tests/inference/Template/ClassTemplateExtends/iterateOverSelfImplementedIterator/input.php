<?php
class O {}
class Foo extends O {}

/**
 * @template-implements Iterator<int, Foo>
 */
class FooCollection implements Iterator {
    private function iterate() : void {
        foreach ($this as $foo) {}
    }
    public function current() { return new Foo(); }
    public function key(): int { return 0; }
    public function next(): void {}
    public function rewind(): void {}
    public function valid(): bool { return false; }
}