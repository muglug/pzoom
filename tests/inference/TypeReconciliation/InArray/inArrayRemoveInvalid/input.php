<?php
function x(?string $foo, int $bar): void {
    if (!in_array($foo, [$bar], true)) {
        throw new Exception();
    }
}
