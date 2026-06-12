<?php // --taint-analysis
/** @psalm-suppress MissingParamType */
function fetch($id): string
{
    return query("SELECT * FROM table WHERE id=" . (int)$id);
}
/**
 * @return string
 * @psalm-taint-sink sql $sql
 * @psalm-taint-specialize
 */
function query(string $sql) {}
$value = $_GET["value"];
$result = fetch($value);
