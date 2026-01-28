<?php
/** @return non-empty-list<non-falsy-string> */
function returnsList(): array {
    if (!isset($http_response_header)) {
        throw new \RuntimeException();
    }
    return $http_response_header;
}
            
