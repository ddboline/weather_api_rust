#!/bin/bash

cd weather_api_wasm;
rm -rf dist/*;

cargo update;
cargo install trunk;
trunk build --release;
cd dist;
sd '/weather_api_wasm' '/wasm_index/weather_api_wasm' index.html;
rm -rf ~/public_html/wasm_index/*;
cp -a * ~/public_html/wasm_index/;

cd ../../weather_app_wasm;
rm -rf dist/*
trunk build --release;
cd dist;
sd '/weather_app_wasm' '/wasm_weather/weather_app_wasm' index.html;
rm -rf ~/public_html/wasm_weather/*;
cp -a * ~/public_html/wasm_weather/;
