<?php
class Foo {
    /**
     * @return int[]
     */
    public function doFoo() {
        return [1, 2, 3];
    }
}

class Bar extends Foo {
    public function doFoo(): array {
        return [4, 5, 6];
    }
}

$b = (new Bar)->doFoo();
