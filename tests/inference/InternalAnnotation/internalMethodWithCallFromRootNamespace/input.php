<?php
                    namespace A {
                        class Foo {
                            /**
                             * @internal
                             */
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace {
                        \A\Foo::barBar();
                    }
