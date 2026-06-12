<?php
function doesNothing(): void {}

function f(): ?int {
    if (rand(0, 1)) {
        return 3;
    }
    return doesNothing();
}
