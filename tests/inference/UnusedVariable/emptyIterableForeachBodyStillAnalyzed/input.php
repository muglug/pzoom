<?php

class FinallyCtx {
    /** @var array<string, int> */
    public array $assigned_var_ids = [];
    /** @var array<string, string> */
    public array $vars_in_scope = [];
}

class StatementsRunner {
    /** @param list<string> $stmts */
    public function analyze(array $stmts, FinallyCtx $context): void
    {
        $context->assigned_var_ids['x'] = 1;
    }
}

/** @param list<string> $stmts */
function mergeFinally(StatementsRunner $analyzer, FinallyCtx $context, FinallyCtx $finally_context, array $stmts): void
{
    $finally_context->assigned_var_ids = [];

    $analyzer->analyze($stmts, $finally_context);

    /** @var string $var_id */
    foreach ($finally_context->assigned_var_ids as $var_id => $_) {
        if (isset($context->vars_in_scope[$var_id])
            && isset($finally_context->vars_in_scope[$var_id])
        ) {
            echo $var_id;
        }
    }
}
