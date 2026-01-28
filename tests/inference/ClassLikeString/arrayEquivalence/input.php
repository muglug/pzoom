<?php
class A {}
class B {}

$foo = [
    A::class,
    B::class
];

foreach ($foo as $class) {
    if ($class === A::class) {}
}
