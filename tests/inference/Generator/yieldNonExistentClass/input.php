<?php
class T {
    private const FACTORIES = [
        ClassNotExisting::class,
    ];

    function f() : Generator {
        foreach (self::FACTORIES as $f) {
            if (class_exists($f)) {
                yield new $f();
            }
        }
    }
}
