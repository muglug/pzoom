<?php
/**
 * @seal-properties
 */
final class OrganizationObject {

    public function __get(string $key)
    {
        return [];
    }
}

echo (new OrganizationObject)->errors;
