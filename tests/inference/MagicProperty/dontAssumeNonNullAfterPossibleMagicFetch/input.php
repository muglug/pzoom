<?php
/** @psalm-no-seal-properties */
class C {
    public function __get(string $name) : string {
        return "hello";
    }
}

function foo(?C $c) : void {
    echo $c->foo;

    if ($c) {}
}
