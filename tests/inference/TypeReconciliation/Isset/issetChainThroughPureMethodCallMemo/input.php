<?php
class Expr7 {}
class ArrayItem7 {
    /** @var Expr7 */
    public $value;
    public function __construct(Expr7 $v) { $this->value = $v; }
}
class Array7 extends Expr7 {
    /** @var array<ArrayItem7|null> */
    public $items = [];
}
class Arg7 {
    /** @var Expr7 */
    public $value;
    public function __construct(Expr7 $v) { $this->value = $v; }
}
class Call7 {
    /**
     * @psalm-pure
     * @return list<Arg7>
     */
    public function getArgs(): array { return [new Arg7(new Expr7())]; }
}

function takesExpr7(Expr7 $e): void { echo $e::class; }

function fooDeep7(Call7 $expr): void {
    if ($expr->getArgs()[0]->value instanceof Array7
        && isset($expr->getArgs()[0]->value->items[0], $expr->getArgs()[0]->value->items[1])
    ) {
        takesExpr7($expr->getArgs()[0]->value->items[0]->value);
    }
}
