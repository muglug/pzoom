<?php
/**
 * @psalm-assert array{
 *      extensions: array<string, array{
 *          version?: string,
 *          type?: "bundled"|"pecl",
 *          require?: list<string>,
 *          env?: array<string, array{
 *              deps?: list<string>,
 *              buildDeps?: list<string>,
 *              configure?: string
 *          }>
 *      }>
 * } $data
 *
 * @param mixed $data
 */
function assertStructure($data): void {}
