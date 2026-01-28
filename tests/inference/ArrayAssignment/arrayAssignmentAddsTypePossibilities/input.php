<?php
function bar(array $value): void {
    $value["b"] = "hello";
    $value = $value + ["a" => 0];
    if (is_int($value["a"])) {}
}
