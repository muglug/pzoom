<?php
function foo(): void
{
    foreach ([0, 1, 2] as $_i) {
        return;
    }

    throw new \Exception();
}
