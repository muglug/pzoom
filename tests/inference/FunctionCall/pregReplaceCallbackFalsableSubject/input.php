<?php
class Doc15 { public function saveXML(): string|false { return ''; } }
function setContents(string $_s): void {}
function f(Doc15 $doc): void {
    $xml = preg_replace_callback(
        '/x/',
        static fn(array $matches): string => 'y',
        $doc->saveXML(),
    );
    if ($xml === null) {
        throw new RuntimeException('bad');
    }
    setContents($xml);
}
