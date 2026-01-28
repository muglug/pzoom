<?php
class Foo {
    /**
     * @return string[]
     */
    public function aa() : array {
        return [];
    }
}

class Bar extends Foo {
    public function aa() : array {
        return [];
    }
}

class Baz extends Bar {
    public function aa() : array {
        return [];
    }
}
