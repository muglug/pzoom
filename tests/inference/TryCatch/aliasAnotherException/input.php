<?php
namespace UserException;

class UserException extends \Exception {

}

namespace Alias\UserException;

use function class_alias;

class_alias(
    \UserException\UserException::class,
    '\Alias\UserException\UserExceptionAlias'
);

namespace Client;

try {
    throw new \Alias\UserException\UserExceptionAlias();
} catch (\Alias\UserException\UserExceptionAlias $e) {
    // do nothing
}