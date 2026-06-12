<?php

class StringRenderer {
    /** @psalm-taint-specialize */
    public function render(string $x) {
        return $x;
    }
}

/** @psalm-suppress InvalidReturnType */
function stub(): StringRenderer { }

$notEchoed = stub()->render($_GET["untrusted"]);
echo stub()->render("a");
