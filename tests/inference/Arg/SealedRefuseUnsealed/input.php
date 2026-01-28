<?php
/** @param array{test: string} $a */
function a(array $a): string {
    return $a["test"];
}

/** @var array{test: string, ...} */
$unsealed = [];
a($unsealed);
                
