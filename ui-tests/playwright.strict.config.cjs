// Release regression uses the packaged CSP and never accepts retry success.
const config=require('./playwright.config.js');
module.exports={...config,retries:0,use:{...config.use,bypassCSP:false}};
