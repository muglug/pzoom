<?php
/**
 * @template TCandidateKey as array-key
 * @param array $arr
 * @param TCandidateKey $key
 * @psalm-assert has-array-key<TCandidateKey> $arr
 */
function keyExists(array $arr, $key) : void {
    if (!array_key_exists($key, $arr)) {
        throw new \Exception("bad");
    }
}

function fromArray(array $data) : void {
    keyExists($data, "id");
    if (is_string($data["id"])) {}
}
