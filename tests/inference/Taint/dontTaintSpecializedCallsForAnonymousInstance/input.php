<?php

class StringRenderer {
    /** @psalm-taint-specialize */
    public function render(string $x) {
        return $x;
    }
}

$notEchoed = (new StringRenderer())->render($_GET["untrusted"]);
echo (new StringRenderer())->render("a");
