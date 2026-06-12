<?php
function noOp(): void {
    return;
}

function doAThing(): bool {
    try {
        noOp();
    } finally {
        return true;
    }

    return false;
}
