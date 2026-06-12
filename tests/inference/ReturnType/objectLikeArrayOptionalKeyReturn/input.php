<?php
/** @return array{a: int, b?: int} */
function foo() : array {
    return rand(0, 1) ? ["a" => 1, "b" => 2] : ["a" => 2];
}
