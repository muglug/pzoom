<?php

final class Arg { public string $value = ''; }

final class Call {
    /**
     * @psalm-pure
     * @return list<Arg>
     */
    public function getArgs(): array { return []; }
}

/**
 * A pure no-arg method call is memoized under its expression id, so narrowing
 * one `$call->getArgs()[n]` carries to later `$call->getArgs()` re-reads. The
 * memoization intersects the fresh `list<Arg>` return with the narrowed list
 * shape; a generic-list ∩ list-shape intersection must keep the shape (not
 * collapse to a generic array), so the offsets below are safe under
 * ensureArrayIntOffsetsExist — matching Psalm.
 */
function viaIsset(Call $call): void {
    if (isset($call->getArgs()[1])) {
        echo $call->getArgs()[0]->value;
        echo $call->getArgs()[1]->value;
    }
}

function viaCount(Call $call): void {
    if (count($call->getArgs()) > 2) {
        echo $call->getArgs()[0]->value;
        echo $call->getArgs()[1]->value;
        echo $call->getArgs()[2]->value;
    }
}
