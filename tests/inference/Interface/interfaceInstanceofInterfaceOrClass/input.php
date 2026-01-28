<?php
interface A {}
class B extends Exception {}

function foo(Throwable $e): void {
    if ($e instanceof A || $e instanceof B) {
        return;
    }

    return;
}

class C extends Exception {}
interface D {}

function bar(Throwable $e): void {
    if ($e instanceof C || $e instanceof D) {
        return;
    }

    return;
}
