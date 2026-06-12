<?php
class Foo {
    public function &foo(): iterable {
        return [] + [1, 2];
    }
}
