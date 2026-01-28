<?php
/** @param array{test: string} $a */
function a(array $a): string {
    return $a["test"];
}

$sealedExtraKeys = ["test" => "str", "somethingElse" => "test"];
a($sealedExtraKeys);
                
