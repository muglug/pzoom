<?php
class A {
    public array $parts = [];
}

class FuncCall {
    /** @var ?A */
    public $name;
    /** @var array<string> */
    public $args = [];
}

function barr(FuncCall $function) : void {
    if (!$function->name instanceof A) {
        return;
    }

    if ($function->name->parts === ["function_exists"]
        && isset($function->args[0])
    ) {
        // do something
    } elseif ($function->name->parts === ["class_exists"]
        && isset($function->args[0])
    ) {
        // do something else
    }
}