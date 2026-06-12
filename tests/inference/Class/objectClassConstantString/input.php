<?php
class Foo {}
function f(Foo $e): string {
    $parts = explode('\\', $e::class);
    $name = array_pop($parts);
    return $name ?? "";
}
