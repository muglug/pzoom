<?php

/**
 * @return iterable<string, array{code: string, error_levels?: list<string>, error_message: string, php_version?: string}|array{code: string, error_message: string, ignored_issues?: list<string>, php_version?: string}>
 */
function providerA(): iterable
{
    return [
        'plain' => [
            'code' => '<?php echo 1;',
            'error_message' => 'Foo',
        ],
        'withIgnored' => [
            'code' => '<?php echo 2;',
            'error_message' => 'Bar',
            'ignored_issues' => [],
            'php_version' => '8.1',
        ],
        'withLevels' => [
            'code' => '<?php echo 3;',
            'error_levels' => [],
            'error_message' => 'Baz',
            'php_version' => '8.2',
        ],
    ];
}
