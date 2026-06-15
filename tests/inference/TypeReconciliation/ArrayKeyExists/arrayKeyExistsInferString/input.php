<?php
function foo(mixed $file) : string {
    if (array_key_exists($file, ["a" => 1, "b" => 2])) {
        return $file;
    }

    return "";
}