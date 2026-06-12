<?php
/** @return array{a: int, b?: int} */
function foo() : array {
    if (rand(0, 1)) {
        return ["a" => 1, "b" => 2];
    }

    return ["a" => 2];
}
