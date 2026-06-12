<?php
abstract class Expr {}

final class Arg {
    public Expr $value;
    public function __construct(Expr $v) { $this->value = $v; }
}

final class Stmt {
    /** @var array<int, Arg> */
    public array $args = [];
}

function handle(Stmt $stmt): int {
    if (!isset($stmt->args[1])) {
        return 1;
    } elseif (isset($stmt->args[1]->value)
        && $stmt->args[1]->value instanceof Expr
        && rand(0, 1) === 1) {
        return 0;
    }
    return 2;
}
