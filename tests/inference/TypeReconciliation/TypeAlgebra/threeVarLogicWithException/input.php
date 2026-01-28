<?php
function foo(?string $a, ?string $b, ?string $c): void {
    if ($a !== null || $b !== null || $c !== null) {
        if ($c !== null) {
            throw new \Exception("bad");
        }

        if ($a !== null) {
            $d = $a;
        } elseif ($b !== null) {
            $d = $b;
        } else {
            $d = $c;
        }
    }
}
