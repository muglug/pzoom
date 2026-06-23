<?php
// Regression for the return-type prototype's trait localization.
//
// A trait method whose `@return TParams` is bound by the using class to a
// fixed-length list (`list{int, int}`) must still accept a wider inferred return
// (`list{int, int, ...<int>}`, produced by a foreach key-write that drops the
// length). The trait body is generic over TParams, so return_analyzer localizes
// the declared return to the template's `as` bound (`list<int>`) — matching
// Psalm — rather than the concrete `@use` binding, which would spuriously reject
// the wider type. Mirrors src/Psalm/Type/Atomic/GenericTrait.php; before the
// localization this emitted a false InvalidReturnStatement.
/**
 * @template TParams of list<int>
 */
trait GenericLike {
    /**
     * @param TParams $params
     * @return TParams
     */
    public function bump(array $params): array {
        foreach ($params as $offset => $tp) {
            $params[$offset] = $tp;
        }
        return $params;
    }
}

final class Concrete {
    /** @use GenericLike<list{int, int}> */
    use GenericLike;
}
