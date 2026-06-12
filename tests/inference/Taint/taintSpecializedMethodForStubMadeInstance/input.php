<?php
/** @psalm-taint-specialize */
class Unsafe {
    public function isUnsafe() {
        return $_GET["unsafe"];
    }
}

/** @psalm-suppress InvalidReturnType */
function stub(): Unsafe { }

/** @psalm-suppress MixedArgument */
echo stub()->isUnsafe();
