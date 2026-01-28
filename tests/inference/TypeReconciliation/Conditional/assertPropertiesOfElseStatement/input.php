<?php
class C {
    public string $a = "";
    public string $b = "";
}

function testElse(C $obj) : void {
    if ($obj->a === "foo") {
    } elseif ($obj->b === "bar") {
    } else if ($obj->b === "baz") {}

    if ($obj->b === "baz") {}
}