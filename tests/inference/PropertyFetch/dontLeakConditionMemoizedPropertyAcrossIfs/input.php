<?php
class LInt { public int $value = 0; }
class LStr { public string $value = ""; }

function f(object $o): bool {
    if ($o instanceof LInt && $o->value !== 0) {
        return true;
    }
    if ($o instanceof LStr && $o->value !== '') {
        return true;
    }
    return false;
}
