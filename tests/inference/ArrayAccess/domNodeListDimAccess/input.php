<?php
function firstFilesElement(DOMDocument $doc): ?DOMElement {
    $list = $doc->getElementsByTagName('files');
    return $list[0];
}

function withTemplate(string $code, string $file, int $line): string {
    return strtr($code, ['%f' => $file, '%l' => $line]);
}
