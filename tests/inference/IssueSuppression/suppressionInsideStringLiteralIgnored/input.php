<?php

/** @return array<string, array{code: string}> */
function providerCases(): array {
    return [
        'embeddedSuppression' => [
            'code' => '<?php
                class Foo {
                    /**
                     * @psalm-suppress UnusedPsalmSuppress
                     */
                    public string $bar = "baz";
                }
            ',
        ],
    ];
}
