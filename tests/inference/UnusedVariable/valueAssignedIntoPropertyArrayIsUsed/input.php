<?php

class Ctx {
    /** @var array<string, string> */
    public $vars_in_scope = [];
}

/** @param array<string, string> $vars */
function patternA(array $vars, Ctx $view_context): Ctx
{
    foreach ($vars as $var => $type) {
        $view_context->vars_in_scope[str_replace('$this->', '$', $var)] = $type;
    }
    return $view_context;
}
