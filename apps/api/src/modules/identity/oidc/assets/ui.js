// FloatCTF 账号页面的渐进增强（同源自托管；JS 关闭时页面与表单照常可用）。
// 1. 头像 / Logo 加载失败时隐藏图片，露出下层的首字母 / 文字标识（不出现破图）；
// 2. 表单提交后忽略重复提交（服务端仍有一次性事务与并发保护）。
(function () {
  "use strict";
  function hide(img) {
    img.hidden = true;
    img.setAttribute("data-failed", "1");
  }
  var images = document.querySelectorAll("img[data-fallback]");
  for (var i = 0; i < images.length; i++) {
    var img = images[i];
    if (img.complete && img.naturalWidth === 0) {
      hide(img);
    } else {
      img.addEventListener("error", function (event) { hide(event.currentTarget); }, { once: true });
    }
  }
  var forms = document.querySelectorAll("form");
  for (var j = 0; j < forms.length; j++) {
    forms[j].addEventListener("submit", function (event) {
      var form = event.currentTarget;
      if (form.getAttribute("data-submitted") === "1") {
        event.preventDefault();
        return;
      }
      form.setAttribute("data-submitted", "1");
      form.setAttribute("aria-busy", "true");
    });
  }
  // 从往返缓存（后退）恢复的页面要允许再次提交。
  window.addEventListener("pageshow", function (event) {
    if (!event.persisted) return;
    for (var k = 0; k < forms.length; k++) {
      forms[k].removeAttribute("data-submitted");
      forms[k].removeAttribute("aria-busy");
    }
  });
})();
