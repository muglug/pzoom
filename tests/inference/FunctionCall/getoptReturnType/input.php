<?php
/** @param array<string, false|list<false|string>|string> $options */
function takesOptions(array $options): void {}

function f(): void {
    $options = getopt('abc', ['def']);
    if ($options === false) {
        return;
    }
    if (!array_key_exists('debug', $options)) {
        $options['debug'] = false;
    }
    takesOptions($options);
}
