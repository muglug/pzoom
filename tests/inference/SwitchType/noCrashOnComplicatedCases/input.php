<?php
class A {}
class B {}
class C {}

/**
 * @psalm-suppress MixedAssignment
 * @psalm-suppress MixedArrayAccess
 */
function foo(array $columns) : bool
{
    foreach ($columns as $c) {
        switch (true) {
            case isset($c["a"]) || $c["b"] || $c["c"]:
            case $c["t"] instanceof A && rand(0, 1):
            case $c["t"] instanceof B && rand(0, 1):
            case $c["t"] instanceof C && rand(0, 1):
                return false;
        }
    }

    return true;
}
