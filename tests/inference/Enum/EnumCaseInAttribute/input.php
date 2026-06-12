<?php
class CreateController {
    #[Param(paramType: ParamType::FLAG)]
    public function actionGet(): void {}
}

use Attribute;

#[Attribute(Attribute::TARGET_METHOD | Attribute::IS_REPEATABLE)]
class Param {
    public function __construct(
        public ParamType $paramType = ParamType::PARAM
    ) {
    }
}

enum ParamType {
    case FLAG;
    case PARAM;
}
