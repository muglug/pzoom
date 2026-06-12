<?php
abstract class Node9 {}
class String9 extends Node9 { public string $value = ''; }
class Arg9 { public Node9 $value; public function __construct(Node9 $v) { $this->value = $v; } }
class FuncCall9 {
    /** @return list<Arg9> */
    public function getArgs(): array { return []; }
}
function isInternal(FuncCall9 $function): bool {
    if (isset($function->getArgs()[0])
        && $function->getArgs()[0]->value instanceof String9
        && function_exists($function->getArgs()[0]->value->value)
    ) {
        $r = new \ReflectionFunction($function->getArgs()[0]->value->value);
        return $r->isInternal();
    }
    return false;
}
