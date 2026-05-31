<?php

final class Context
{
    public bool $collect_initializations = false;
    public bool $collect_mutations = false;
}

function analyze(Context $context): void
{
    // Inside the body of `if (A || B)`, neither A nor B is individually known:
    // `$context->collect_initializations` is still `bool` (it could be false
    // here when `$context->collect_mutations` is the truthy operand), so the
    // nested `if` must NOT be reported as redundant/contradictory.
    if ($context->collect_initializations || $context->collect_mutations) {
        if ($context->collect_initializations) {
            echo "init\n";
        }
    }
}
