<?php
interface Foo {
    public function &foo(): int;
}

class Bar implements Foo {
    private int $x = 0;
    public function &foo(): int {
        return $this->x;
    }
}
