<?php
/** @param array{test: string} $a */
function a(array $a): string {
    return $a["test"];
}

$sealed = ["test" => "str"];
a($sealed);
                
