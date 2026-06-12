<?php

abstract class ScalarAtomic {}
class TStringAtomic extends ScalarAtomic {}
class TIntAtomic extends ScalarAtomic {}

function compareParts(ScalarAtomic $input, ScalarAtomic $container): bool {
    if ($input instanceof TStringAtomic && $container instanceof TStringAtomic) {
        return true;
    }
    if ($container instanceof TIntAtomic && $input instanceof ScalarAtomic) {
        return true;
    }
    return false;
}

function stillReportsPlain(ScalarAtomic $input): bool {
    if ($input instanceof ScalarAtomic) {
        return true;
    }
    return false;
}

function stillReportsAfterUnitNegation(ScalarAtomic $input): bool {
    if ($input instanceof TStringAtomic) {
        return false;
    }
    if ($input instanceof ScalarAtomic) {
        return true;
    }
    return false;
}
