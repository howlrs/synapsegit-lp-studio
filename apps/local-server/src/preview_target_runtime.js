(function previewTargetRuntime(
  editorOrigin,
  projectId,
  snapshotId,
  revisionId,
  pagePath,
) {
  "use strict";

  const call = Function.call.bind(Function.call);
  const getAttribute = Function.call.bind(Element.prototype.getAttribute);
  const getBoundingClientRect = Function.call.bind(
    Element.prototype.getBoundingClientRect,
  );
  const closest = Function.call.bind(Element.prototype.closest);
  const preventDefault = Function.call.bind(Event.prototype.preventDefault);
  const stopPropagation = Function.call.bind(Event.prototype.stopPropagation);
  const rangeRect = Function.call.bind(Range.prototype.getBoundingClientRect);
  const rangeText = Function.call.bind(Range.prototype.toString);
  const textContentGetter = Object.getOwnPropertyDescriptor(
    Node.prototype,
    "textContent",
  )?.get;
  const getTextContent = textContentGetter
    ? Function.call.bind(textContentGetter)
    : () => "";
  const getSelection = window.getSelection.bind(window);
  const getComputedStyle = window.getComputedStyle.bind(window);
  const elementFromPoint = document.elementFromPoint.bind(document);
  const elementsFromPoint = document.elementsFromPoint.bind(document);
  const randomUuid = crypto.randomUUID.bind(crypto);
  const postToEditor = parent.postMessage.bind(parent);
  const minimum = Math.min.bind(Math);
  const maximum = Math.max.bind(Math);
  const round = Math.round.bind(Math);
  const finite = Number.isFinite;
  const encoder = new TextEncoder();
  const blockSelector =
    "header,nav,main,section,article,aside,footer,[role='banner'],[role='navigation'],[role='main'],[role='region'],[role='complementary'],[role='contentinfo']";
  const handles = new WeakMap();
  const nodesByHandle = new Map();
  const blockElements = new Set();
  const targetKinds = new Set([
    "page",
    "block",
    "element",
    "text",
    "point",
    "region",
  ]);
  let channelId = "";
  let mode = "interact";
  let targetKind = "element";
  let previewScale = 1;
  let layoutEpoch = 0;
  let drag = null;
  let overlay = null;

  const boundedText = (raw, maxBytes = 240) => {
    const normalized = String(raw ?? "")
      .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, "")
      .replace(/(?:ghp_|github_pat_|sk-)[A-Za-z0-9_-]{12,}/g, "[redacted]")
      .replace(/AKIA[A-Z0-9]{12,}/g, "[redacted]")
      .replace(/Bearer\s+[A-Za-z0-9._~-]{12,}/gi, "Bearer [redacted]")
      .replace(/\s+/g, " ")
      .trim();
    let result = "";
    for (const character of normalized) {
      const next = result + character;
      if (encoder.encode(next).byteLength > maxBytes) break;
      result = next;
    }
    return result;
  };

  const clamp = (value, low, high) => minimum(high, maximum(low, value));
  const validChannel = (value) =>
    typeof value === "string" && value.length > 0 && value.length <= 128;
  const isVisible = (element) => {
    const rect = getBoundingClientRect(element);
    const style = getComputedStyle(element);
    return (
      rect.width >= 8 &&
      rect.height >= 8 &&
      style.display !== "none" &&
      style.visibility !== "hidden"
    );
  };
  const elementDepth = (element) => {
    let depth = 0;
    let current = element;
    while (current.parentElement && depth < 32) {
      depth += 1;
      current = current.parentElement;
    }
    return depth;
  };
  const handleFor = (element) => {
    let handle = handles.get(element);
    if (!handle) {
      handle = "node_" + randomUuid().replaceAll("-", "");
      handles.set(element, handle);
      nodesByHandle.set(handle, element);
    }
    return handle;
  };
  const directChildren = (element) => Array.from(element.children);
  const labelFor = (element) => {
    const explicit =
      getAttribute(element, "data-lp-label") ||
      getAttribute(element, "aria-label") ||
      getAttribute(element, "alt") ||
      getAttribute(element, "title");
    const text = explicit || getTextContent(element) || element.tagName;
    return boundedText(text) || element.tagName.toLowerCase();
  };
  const uniqueIdentifier = (element) =>
    boundedText(
      getAttribute(element, "data-lp-id") ||
        getAttribute(element, "data-studio-block") ||
        getAttribute(element, "id"),
      128,
    ) || undefined;
  const domPath = (element) => {
    const segments = [];
    let current = element;
    while (current instanceof Element && segments.length < 8) {
      let index = 1;
      let sibling = current.previousElementSibling;
      while (sibling) {
        if (sibling.tagName === current.tagName) index += 1;
        sibling = sibling.previousElementSibling;
      }
      segments.unshift(
        current.tagName.toLowerCase() + ":nth-of-type(" + index + ")",
      );
      current = current.parentElement;
    }
    return boundedText(segments.join(" > "), 512) || undefined;
  };
  const ancestorFingerprint = (element) => {
    const parts = [];
    let current = element;
    while (current instanceof Element && parts.length < 5) {
      const role = boundedText(getAttribute(current, "role"), 48);
      const identifier = uniqueIdentifier(current);
      parts.unshift(
        current.tagName.toLowerCase() +
          (role ? "[role=" + role + "]" : "") +
          (identifier ? "#" + identifier : ""),
      );
      current = current.parentElement;
    }
    return boundedText(parts.join("/"), 512) || undefined;
  };
  const siblingIndex = (element) => {
    const parentElement = element.parentElement;
    if (!parentElement) return 0;
    return maximum(0, directChildren(parentElement).indexOf(element));
  };
  const elementAnchor = (element) => {
    const anchor = {
      tagName: element.tagName.toLowerCase(),
      domPath: domPath(element),
      ancestorFingerprint: ancestorFingerprint(element),
      siblingIndex: siblingIndex(element),
    };
    const identifier = uniqueIdentifier(element);
    const role = boundedText(getAttribute(element, "role"), 64);
    const name = labelFor(element);
    const classTokens = Array.from(element.classList)
      .map((value) => boundedText(value, 64))
      .filter(Boolean)
      .slice(0, 8);
    if (identifier) anchor.uniqueElementId = identifier;
    if (role) anchor.role = role;
    if (name) anchor.accessibleName = name;
    if (classTokens.length > 0) anchor.classTokens = classTokens;
    return anchor;
  };
  const viewportCapture = () => ({
    cssWidth: maximum(1, window.innerWidth),
    cssHeight: maximum(1, window.innerHeight),
    scrollX: clamp(window.scrollX, 0, 1_000_000),
    scrollY: clamp(window.scrollY, 0, 1_000_000),
    devicePixelRatio: clamp(window.devicePixelRatio || 1, 0.1, 16),
    visualViewportScale: clamp(window.visualViewport?.scale || 1, 0.1, 10),
    previewScale,
  });
  const documentCapture = () => ({
    cssWidth: maximum(1, document.documentElement.scrollWidth),
    cssHeight: maximum(1, document.documentElement.scrollHeight),
    layoutEpoch,
  });
  const rectGeometry = (rawRect) => {
    const width = maximum(0, rawRect.width);
    const height = maximum(0, rawRect.height);
    const viewportWidth = maximum(1, window.innerWidth);
    const viewportHeight = maximum(1, window.innerHeight);
    const clippedLeft = clamp(rawRect.x, 0, viewportWidth);
    const clippedTop = clamp(rawRect.y, 0, viewportHeight);
    const clippedRight = clamp(rawRect.x + width, 0, viewportWidth);
    const clippedBottom = clamp(rawRect.y + height, 0, viewportHeight);
    return {
      documentCssPixelRect: {
        x: rawRect.x + window.scrollX,
        y: rawRect.y + window.scrollY,
        width,
        height,
      },
      viewportCssPixelRect: { x: rawRect.x, y: rawRect.y, width, height },
      viewportNormalizedRect: {
        x: clippedLeft / viewportWidth,
        y: clippedTop / viewportHeight,
        width: maximum(0, clippedRight - clippedLeft) / viewportWidth,
        height: maximum(0, clippedBottom - clippedTop) / viewportHeight,
      },
    };
  };
  const currentPagePath = () => pagePath;
  const commonTarget = (kind, label) => ({
    schemaVersion: 1,
    targetId: "tgt_" + randomUuid().replaceAll("-", ""),
    captureRevisionId: revisionId,
    captureSource: snapshotId === revisionId ? "accepted" : "proposal",
    ...(snapshotId === revisionId ? {} : { captureProposalId: snapshotId }),
    pagePath: currentPagePath(),
    kind,
    label: boundedText(label) || kind,
    viewport: viewportCapture(),
    document: documentCapture(),
  });
  const post = (type, payload) => {
    if (!validChannel(channelId)) return;
    postToEditor(
      {
        type,
        schemaVersion: "1",
        channelId,
        projectId,
        snapshotId,
        revisionId,
        ...payload,
      },
      editorOrigin,
    );
  };

  const classifyBlock = (element) => {
    const tagName = element.tagName.toLowerCase();
    if (
      [
        "header",
        "nav",
        "main",
        "section",
        "article",
        "aside",
        "footer",
      ].includes(tagName)
    ) {
      return "semantic";
    }
    if (getAttribute(element, "role")) return "landmark";
    return "heuristic";
  };
  const collectBlocks = () => {
    const candidates = new Set(document.querySelectorAll(blockSelector));
    for (const root of [document.body, document.querySelector("main")]) {
      if (!root) continue;
      for (const child of directChildren(root)) candidates.add(child);
    }
    blockElements.clear();
    for (const element of candidates) {
      const rect = getBoundingClientRect(element);
      if (
        element instanceof Element &&
        isVisible(element) &&
        rect.width * rect.height >= 1600 &&
        elementDepth(element) <= 16
      ) {
        blockElements.add(element);
      }
      if (blockElements.size >= 128) break;
    }
    return Array.from(blockElements);
  };
  const containingBlock = (element) => {
    let current = element;
    while (current instanceof Element) {
      if (blockElements.has(current)) return current;
      current = current.parentElement;
    }
    return document.body;
  };
  const visibleSibling = (element, direction) => {
    let current =
      direction < 0
        ? element.previousElementSibling
        : element.nextElementSibling;
    while (current) {
      if (isVisible(current)) return current;
      current =
        direction < 0
          ? current.previousElementSibling
          : current.nextElementSibling;
    }
    return null;
  };
  const layoutMode = (element) => {
    const style = getComputedStyle(element);
    if (style.position !== "static") return "positioned";
    if (style.display.includes("grid")) return "grid";
    if (style.display.includes("flex")) return "flex";
    return style.display === "block" || style.display === "flow-root"
      ? "flow"
      : "unknown";
  };
  const regionContext = (x, y, width = 0, height = 0) => {
    collectBlocks();
    const halfWidth = width / 2;
    const halfHeight = height / 2;
    const points = [
      [x, y],
      [
        clamp(x - halfWidth, 0, window.innerWidth - 1),
        clamp(y - halfHeight, 0, window.innerHeight - 1),
      ],
      [
        clamp(x + halfWidth, 0, window.innerWidth - 1),
        clamp(y - halfHeight, 0, window.innerHeight - 1),
      ],
      [
        clamp(x - halfWidth, 0, window.innerWidth - 1),
        clamp(y + halfHeight, 0, window.innerHeight - 1),
      ],
      [
        clamp(x + halfWidth, 0, window.innerWidth - 1),
        clamp(y + halfHeight, 0, window.innerHeight - 1),
      ],
    ];
    const hits = points.flatMap(([pointX, pointY]) =>
      elementsFromPoint(pointX, pointY),
    );
    const primary =
      hits.find((candidate) => candidate instanceof Element) || document.body;
    const block = containingBlock(primary);
    const previous = visibleSibling(primary, -1);
    const next = visibleSibling(primary, 1);
    return {
      containingBlock: elementAnchor(block),
      ...(previous ? { previousVisibleSibling: elementAnchor(previous) } : {}),
      ...(next ? { nextVisibleSibling: elementAnchor(next) } : {}),
      layoutMode: layoutMode(block),
    };
  };

  const clearOverlay = () => {
    overlay?.remove();
    overlay = null;
    drag = null;
  };
  const showOverlay = (rect) => {
    if (!overlay) {
      overlay = document.createElement("div");
      overlay.setAttribute("data-synapsegit-preview-overlay", "true");
      overlay.style.cssText =
        "position:fixed;z-index:2147483647;pointer-events:none;border:2px solid #6dff9d;background:rgb(109 255 157 / 18%);box-shadow:0 0 0 1px #10291f;";
      document.documentElement.append(overlay);
    }
    overlay.style.left = rect.x + "px";
    overlay.style.top = rect.y + "px";
    overlay.style.width = rect.width + "px";
    overlay.style.height = rect.height + "px";
  };
  const postTarget = (target) => {
    post("synapsegit-lp.target-draft", { target });
  };
  const capturePage = () => {
    clearOverlay();
    postTarget(commonTarget("page", "ページ全体"));
  };
  const captureElement = (kind, rawElement) => {
    collectBlocks();
    const element = kind === "block" ? containingBlock(rawElement) : rawElement;
    if (!(element instanceof Element) || !isVisible(element)) return;
    const rect = getBoundingClientRect(element);
    const target = {
      ...commonTarget(kind, labelFor(element)),
      geometry: rectGeometry(rect),
      elementAnchor: elementAnchor(element),
      ...(kind === "block"
        ? {
            block: {
              source: classifyBlock(element),
              level: minimum(16, elementDepth(element)),
            },
          }
        : {}),
    };
    showOverlay({
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    });
    postTarget(target);
  };
  const captureTextRange = (range, element) => {
    const rawExact = rangeText(range);
    const exact = boundedText(rawExact, 1024);
    if (!exact || !(element instanceof Element)) return;
    const rawRect = rangeRect(range);
    if (rawRect.width <= 0 || rawRect.height <= 0) return;
    const anchor = { exact };
    if (
      exact === rawExact &&
      range.startContainer === range.endContainer &&
      range.startContainer.nodeType === Node.TEXT_NODE
    ) {
      anchor.startOffset = range.startOffset;
      anchor.endOffset = range.endOffset;
      const whole = String(getTextContent(range.startContainer) || "");
      const prefix = boundedText(
        whole.slice(maximum(0, range.startOffset - 64), range.startOffset),
        128,
      );
      const suffix = boundedText(
        whole.slice(range.endOffset, range.endOffset + 64),
        128,
      );
      if (prefix) anchor.prefix = prefix;
      if (suffix) anchor.suffix = suffix;
    }
    showOverlay({
      x: rawRect.x,
      y: rawRect.y,
      width: rawRect.width,
      height: rawRect.height,
    });
    postTarget({
      ...commonTarget("text", exact),
      geometry: rectGeometry(rawRect),
      elementAnchor: elementAnchor(element),
      textAnchor: anchor,
    });
  };
  const captureTextSelection = () => {
    const selection = getSelection();
    if (!selection || selection.rangeCount !== 1 || selection.isCollapsed)
      return;
    const range = selection.getRangeAt(0);
    const container =
      range.commonAncestorContainer instanceof Element
        ? range.commonAncestorContainer
        : range.commonAncestorContainer.parentElement;
    captureTextRange(range, container);
  };
  const captureElementText = (element) => {
    const selection = getSelection();
    if (!selection) return;
    selection.removeAllRanges();
    const range = document.createRange();
    range.selectNodeContents(element);
    selection.addRange(range);
    captureTextRange(range, element);
  };
  const capturePoint = (rawX, rawY) => {
    const x = clamp(rawX, 0, maximum(0, window.innerWidth - 1));
    const y = clamp(rawY, 0, maximum(0, window.innerHeight - 1));
    showOverlay({ x: x - 5, y: y - 5, width: 10, height: 10 });
    postTarget({
      ...commonTarget("point", "空白を含む座標"),
      point: {
        documentCssPixel: {
          x: clamp(x + window.scrollX, 0, 1_000_000),
          y: clamp(y + window.scrollY, 0, 1_000_000),
        },
        viewportNormalized: {
          x: x / maximum(1, window.innerWidth),
          y: y / maximum(1, window.innerHeight),
        },
      },
      regionAnchor: regionContext(x, y),
    });
  };
  const normalizedRegion = (startX, startY, endX, endY) => {
    let left = clamp(minimum(startX, endX), 0, window.innerWidth);
    let top = clamp(minimum(startY, endY), 0, window.innerHeight);
    let right = clamp(maximum(startX, endX), 0, window.innerWidth);
    let bottom = clamp(maximum(startY, endY), 0, window.innerHeight);
    if (right - left < 8) {
      const center = (left + right) / 2;
      left = clamp(center - 4, 0, maximum(0, window.innerWidth - 8));
      right = minimum(window.innerWidth, left + 8);
    }
    if (bottom - top < 8) {
      const center = (top + bottom) / 2;
      top = clamp(center - 4, 0, maximum(0, window.innerHeight - 8));
      bottom = minimum(window.innerHeight, top + 8);
    }
    return { x: left, y: top, width: right - left, height: bottom - top };
  };
  const captureRegion = (startX, startY, endX, endY) => {
    const rect = normalizedRegion(startX, startY, endX, endY);
    const centerX = rect.x + rect.width / 2;
    const centerY = rect.y + rect.height / 2;
    showOverlay(rect);
    postTarget({
      ...commonTarget("region", "選択した領域"),
      geometry: rectGeometry(rect),
      regionAnchor: regionContext(centerX, centerY, rect.width, rect.height),
    });
  };

  const emitStructure = () => {
    const blocks = collectBlocks();
    const nodes = [];
    for (const block of blocks) {
      const handle = handleFor(block);
      let parent = block.parentElement;
      while (parent && !blockElements.has(parent)) {
        parent = parent.parentElement;
      }
      nodes.push({
        runtimeNodeHandle: handle,
        ...(parent ? { parentRuntimeNodeHandle: handleFor(parent) } : {}),
        kind: "block",
        label: labelFor(block),
        tagName: block.tagName.toLowerCase(),
        depth: minimum(16, elementDepth(block)),
      });
      if (nodes.length >= 200) break;
    }
    const emittedHandles = new Set(nodes.map((node) => node.runtimeNodeHandle));
    for (const element of Array.from(
      document.querySelectorAll("h1,h2,h3,p,a,button,img"),
    )) {
      if (!isVisible(element) || nodes.length >= 200) continue;
      const handle = handleFor(element);
      if (emittedHandles.has(handle)) continue;
      const parent = containingBlock(element);
      nodes.push({
        runtimeNodeHandle: handle,
        parentRuntimeNodeHandle: handleFor(parent),
        kind: "element",
        label: labelFor(element),
        tagName: element.tagName.toLowerCase(),
        depth: minimum(16, elementDepth(element)),
      });
      emittedHandles.add(handle);
    }
    post("synapsegit-lp.structure", { nodes: nodes.slice(0, 200) });
  };

  addEventListener("message", (event) => {
    const message = event.data;
    if (
      event.origin !== editorOrigin ||
      event.source !== parent ||
      !message ||
      message.type !== "synapsegit-lp.action" ||
      message.schemaVersion !== "1" ||
      message.projectId !== projectId ||
      message.snapshotId !== snapshotId ||
      message.revisionId !== revisionId ||
      !validChannel(message.channelId)
    ) {
      return;
    }
    if (message.action === "set_mode") {
      if (message.mode !== "select" && message.mode !== "interact") return;
      if (!targetKinds.has(message.targetKind)) return;
      channelId = message.channelId;
      mode = message.mode;
      targetKind = message.targetKind;
      previewScale =
        finite(message.previewScale) &&
        message.previewScale > 0 &&
        message.previewScale <= 8
          ? message.previewScale
          : 1;
      clearOverlay();
      emitStructure();
    } else if (message.action === "clear_selection") {
      if (channelId === message.channelId) clearOverlay();
    } else if (message.action === "request_structure") {
      channelId = message.channelId;
      emitStructure();
    } else if (message.action === "capture_page") {
      channelId = message.channelId;
      capturePage();
    } else if (message.action === "capture_node") {
      channelId = message.channelId;
      const element = nodesByHandle.get(message.runtimeNodeHandle);
      if (!(element instanceof Element) || !targetKinds.has(message.targetKind))
        return;
      if (message.targetKind === "page") capturePage();
      else if (message.targetKind === "text") captureElementText(element);
      else if (message.targetKind === "point") {
        const rect = getBoundingClientRect(element);
        capturePoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
      } else if (message.targetKind === "region") {
        const rect = getBoundingClientRect(element);
        captureRegion(
          rect.x,
          rect.y,
          rect.x + rect.width,
          rect.y + rect.height,
        );
      } else captureElement(message.targetKind, element);
    }
  });

  addEventListener(
    "click",
    (event) => {
      if (
        mode !== "select" ||
        !validChannel(channelId) ||
        targetKind === "region"
      )
        return;
      const element = event.target instanceof Element ? event.target : null;
      if (!element) return;
      preventDefault(event);
      stopPropagation(event);
      if (targetKind === "page") capturePage();
      else if (targetKind === "point")
        capturePoint(event.clientX, event.clientY);
      else if (targetKind === "text") captureTextSelection();
      else captureElement(targetKind, element);
    },
    true,
  );
  addEventListener(
    "pointerdown",
    (event) => {
      if (mode !== "select" || targetKind !== "region" || event.button !== 0)
        return;
      preventDefault(event);
      stopPropagation(event);
      drag = { x: event.clientX, y: event.clientY };
      showOverlay(normalizedRegion(drag.x, drag.y, drag.x, drag.y));
    },
    true,
  );
  addEventListener(
    "pointermove",
    (event) => {
      if (!drag) return;
      preventDefault(event);
      showOverlay(
        normalizedRegion(drag.x, drag.y, event.clientX, event.clientY),
      );
    },
    true,
  );
  addEventListener(
    "pointerup",
    (event) => {
      if (!drag) return;
      preventDefault(event);
      stopPropagation(event);
      const start = drag;
      drag = null;
      captureRegion(start.x, start.y, event.clientX, event.clientY);
    },
    true,
  );
  addEventListener(
    "keydown",
    (event) => {
      if (
        mode !== "select" ||
        event.key !== "Enter" ||
        !validChannel(channelId)
      )
        return;
      const element =
        document.activeElement instanceof Element
          ? document.activeElement
          : document.body;
      const rect = getBoundingClientRect(element);
      preventDefault(event);
      if (targetKind === "page") capturePage();
      else if (targetKind === "point")
        capturePoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
      else if (targetKind === "region")
        captureRegion(
          rect.x,
          rect.y,
          rect.x + rect.width,
          rect.y + rect.height,
        );
      else if (targetKind === "text") captureElementText(element);
      else captureElement(targetKind, element);
    },
    true,
  );

  try {
    new ResizeObserver(() => {
      layoutEpoch += 1;
    }).observe(document.documentElement);
    new MutationObserver(() => {
      layoutEpoch += 1;
    }).observe(document.documentElement, {
      attributes: true,
      childList: true,
      subtree: true,
    });
  } catch {
    layoutEpoch = 0;
  }
});
