<?php
final class Context2 {
    public bool $collect_initializations = false;
    public bool $collect_mutations = false;
}

function methodExists(string $id, ?string $analyzer): bool { return $analyzer !== null && $id !== ''; }

function f(Context2 $context, string $set_method_id, bool $other): void {
    if ($other
        && methodExists(
            $set_method_id,
            !$context->collect_initializations && !$context->collect_mutations ? "analyzer" : null
        )
    ) {
        if (!$context->collect_initializations && !$context->collect_mutations) {
            echo "taint";
        }
    }
}
