<?php
final class Ctx {
    /** @var list<string> */
    public array $reconciled = [];
}
final class EA2 {
    public static function analyze(Ctx $c): bool { $c->reconciled = ['a']; return true; }
}
function f(Ctx $context): void {
    $left = clone $context;
    /** @var list<string> $left->reconciled */
    $left->reconciled = [];

    if (EA2::analyze($left) === false) {
        return;
    }

    if ($left->reconciled) {
        echo count($left->reconciled);
    }
}
