<?php
/**
 * @return array{foo:numeric-string}&array<non-empty-string,non-empty-string>
 */
function defaultQueryParams(): array
{
    return [
       "foo" => "123",
       "bar" => "baz",
    ];
}

/**
 * @return array<non-empty-string, non-empty-string>
 */
function getQueryParams(): array
{
    $queryParams = defaultQueryParams();
    $queryParams["a"] = "zzz";
    return $queryParams;
}
