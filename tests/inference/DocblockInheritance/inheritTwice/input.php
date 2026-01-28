<?php
class Foo {
    /**
     * @return string[]
     */
    public function aa() {
        return [];
    }
}

class Bar extends Foo {
    public function aa() {
        return [];
    }
}

class Baz extends Bar {
    public function aa() {
        return [];
    }
}
