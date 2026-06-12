<?php

class NameNode {
    public string $name = '';
}

class Arg {
    public ?NameNode $name = null;
}

class Param {
    public string $name = '';
}

/** @param list<Param> $params */
function findParam(Arg $arg, array $params): ?Param {
    if ($arg->name !== null) {
        return array_reduce(
            $params,
            static function (?Param $carry, Param $param) use ($arg) {
                if ($param->name === $arg->name->name) {
                    return $param;
                }
                return $carry;
            },
            null,
        );
    }
    return null;
}
