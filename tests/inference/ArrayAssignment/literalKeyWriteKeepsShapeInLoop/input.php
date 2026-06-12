<?php

/** @param array{byRef: bool, name?: string, refMode: 'rw'|'w'|'r', variadic: bool, optional: bool, type: string} $entry */
function assertEntryShape(array $entry): void {
    echo $entry['type'];
}

/** @param array<string, string> $entryParameters */
function normalizeEntries(array $entryParameters, string $wanted): void {
    /**
     * @var array<string, array{byRef: bool, refMode: 'rw'|'w'|'r', variadic: bool, optional: bool, type: string}>
     */
    $normalizedEntries = [];

    foreach ($entryParameters as $key => $entry) {
        $normalizedKey = $key;
        /**
         * @var array{byRef: bool, refMode: 'rw'|'w'|'r', variadic: bool, optional: bool, type: string} $normalizedEntry
         */
        $normalizedEntry = [
            'variadic' => false,
            'byRef' => false,
            'optional' => false,
            'type' => $entry,
        ];

        do {
            if (strncmp($normalizedKey, '...', 3) === 0) {
                $normalizedEntry['variadic'] = true;
                $normalizedKey = substr($normalizedKey, 3);
                continue;
            }
            break;
        } while (true);

        $normalizedEntry['name'] = $normalizedKey;
        $normalizedEntries[$normalizedKey] = $normalizedEntry;
    }

    if (isset($normalizedEntries[$wanted])) {
        assertEntryShape($normalizedEntries[$wanted]);
    }
}
