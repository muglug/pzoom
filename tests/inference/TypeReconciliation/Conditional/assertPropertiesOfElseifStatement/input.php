<?php
class C {
    public string $a = "";
    public string $b = "";
}

function testElseif(C $obj) : void {
    if ($obj->a === "foo") {
    } elseif ($obj->b === "bar") {
    } elseif ($obj->b === "baz") {}

    if ($obj->b === "baz") {}
}