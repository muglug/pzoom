<?php
class C {
    const A = [
        "b" => ["c" => false],
        "c" => ["c" => true],
        "d" => ["c" => true]
    ];
}

/** @var string */
$s = "b";

if (isset(C::A[$s]["c"]) && C::A[$s]["c"] === false) {}
