<?php
function doesntFindBug(?string $old, ?string $new): void {
    if (empty($old) && empty($new)) {
        return;
    }

    if (($old && empty($new)) || ($new && empty($old))) {
        return;
    }
}
