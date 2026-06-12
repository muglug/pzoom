<?php
abstract class Foo {
    /**
     * @return array|string
     */
    abstract public function getTargets();
}

class Bar extends Foo {
    public function getTargets(): string {
        return "baz";
    }
}

$a = (new Bar)->getTargets();
